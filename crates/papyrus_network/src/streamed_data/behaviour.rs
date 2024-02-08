#[cfg(test)]
#[path = "behaviour_test.rs"]
mod behaviour_test;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use defaultmap::DefaultHashMap;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    NotifyHandler,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId, StreamProtocol};

use super::handler::{
    Handler,
    RequestFromBehaviourEvent,
    RequestToBehaviourEvent,
    SessionError as HandlerSessionError,
};
use super::{
    Config,
    DataBound,
    GenericEvent,
    InboundSessionId,
    OutboundSessionId,
    QueryBound,
    SessionId,
};

#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error("Connection timed out after {} seconds.", session_timeout.as_secs())]
    Timeout { session_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Remote peer doesn't support the {protocol_name} protocol.")]
    #[allow(dead_code)]
    RemoteDoesntSupportProtocol { protocol_name: StreamProtocol },
    #[error("In an inbound session, remote peer sent data after sending the query.")]
    OtherOutboundPeerSentData,
    // If there's a connection with a single session and it was closed because of another reason,
    // we might get ConnectionClosed instead of that reason because the swarm automatically closes
    // a connection that has no sessions. If this is a problem, set the swarm's
    // idle_connection_timeout to a non-zero number.
    #[error("Connection to remote peer closed.")]
    ConnectionClosed,
}

impl<Query: QueryBound, Data: DataBound> From<GenericEvent<Query, Data, HandlerSessionError>>
    for GenericEvent<Query, Data, SessionError>
{
    fn from(event: GenericEvent<Query, Data, HandlerSessionError>) -> Self {
        match event {
            GenericEvent::NewInboundSession { query, inbound_session_id, peer_id } => {
                Self::NewInboundSession { query, inbound_session_id, peer_id }
            }
            GenericEvent::ReceivedData { outbound_session_id, data } => {
                Self::ReceivedData { outbound_session_id, data }
            }
            GenericEvent::SessionFailed {
                session_id,
                error: HandlerSessionError::Timeout { session_timeout },
            } => {
                Self::SessionFailed { session_id, error: SessionError::Timeout { session_timeout } }
            }
            GenericEvent::SessionFailed {
                session_id,
                error: HandlerSessionError::IOError(error),
            } => Self::SessionFailed { session_id, error: SessionError::IOError(error) },
            GenericEvent::SessionFailed {
                session_id,
                error: HandlerSessionError::RemoteDoesntSupportProtocol { protocol_name },
            } => Self::SessionFailed {
                session_id,
                error: SessionError::RemoteDoesntSupportProtocol { protocol_name },
            },
            GenericEvent::SessionFailed {
                session_id,
                error: HandlerSessionError::OtherOutboundPeerSentData,
            } => Self::SessionFailed { session_id, error: SessionError::OtherOutboundPeerSentData },
            GenericEvent::SessionFinishedSuccessfully { session_id } => {
                Self::SessionFinishedSuccessfully { session_id }
            }
        }
    }
}

pub type Event<Query, Data> = GenericEvent<Query, Data, SessionError>;

#[derive(thiserror::Error, Debug)]
#[error("The given session ID doesn't exist.")]
pub struct SessionIdNotFoundError;

#[derive(thiserror::Error, Debug)]
#[error("We are not connected to the given peer. Dial to the given peer and try again.")]
pub struct PeerNotConnected;

// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
pub struct Behaviour<Query: QueryBound, Data: DataBound> {
    config: Config,
    // TODO(shahak): Use deadqueue if using a VecDeque is a bug (libp2p uses VecDeque, so we opened
    // an issue on it https://github.com/libp2p/rust-libp2p/issues/5147)
    pending_events: VecDeque<ToSwarm<Event<Query, Data>, RequestFromBehaviourEvent<Query, Data>>>,
    pending_queries: DefaultHashMap<PeerId, Vec<(Query, OutboundSessionId)>>,
    connection_ids_map: DefaultHashMap<PeerId, HashSet<ConnectionId>>,
    session_id_to_peer_id_and_connection_id: HashMap<SessionId, (PeerId, ConnectionId)>,
    next_outbound_session_id: OutboundSessionId,
    next_inbound_session_id: Arc<AtomicUsize>,
    dropped_sessions: HashSet<SessionId>,
}

impl<Query: QueryBound, Data: DataBound> Behaviour<Query, Data> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            pending_events: Default::default(),
            pending_queries: Default::default(),
            connection_ids_map: Default::default(),
            session_id_to_peer_id_and_connection_id: Default::default(),
            next_outbound_session_id: Default::default(),
            next_inbound_session_id: Arc::new(Default::default()),
            dropped_sessions: Default::default(),
        }
    }

    /// Send query to the given peer and start a new outbound session with it. Return the id of the
    /// new session.
    pub fn send_query(
        &mut self,
        query: Query,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        let connection_id =
            *self.connection_ids_map.get(peer_id).iter().next().ok_or(PeerNotConnected)?;

        let outbound_session_id = self.next_outbound_session_id;
        self.next_outbound_session_id.value += 1;

        self.session_id_to_peer_id_and_connection_id
            .insert(outbound_session_id.into(), (peer_id, connection_id));

        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(connection_id),
            event: RequestFromBehaviourEvent::CreateOutboundSession { query, outbound_session_id },
        });

        Ok(outbound_session_id)
    }

    /// Send a data message to an open inbound session.
    pub fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let (peer_id, connection_id) =
            self.get_peer_id_and_connection_id_from_session_id(inbound_session_id.into())?;
        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(connection_id),
            event: RequestFromBehaviourEvent::SendData { data, inbound_session_id },
        });
        Ok(())
    }

    /// Instruct behaviour to close session. A corresponding SessionFinishedSuccessfully event will
    /// be reported when the session is closed.
    pub fn close_inbound_session(
        &mut self,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let (peer_id, connection_id) =
            self.get_peer_id_and_connection_id_from_session_id(inbound_session_id.into())?;
        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(connection_id),
            event: RequestFromBehaviourEvent::CloseInboundSession { inbound_session_id },
        });
        Ok(())
    }

    /// Instruct behaviour to drop outbound session. The session won't emit any events once dropped.
    /// The other peer will receive an IOError on their corresponding inbound session.
    pub fn drop_session(&mut self, session_id: SessionId) -> Result<(), SessionIdNotFoundError> {
        let (peer_id, connection_id) =
            self.get_peer_id_and_connection_id_from_session_id(session_id)?;
        if self.dropped_sessions.insert(session_id) {
            self.pending_events.push_back(ToSwarm::NotifyHandler {
                peer_id,
                handler: NotifyHandler::One(connection_id),
                event: RequestFromBehaviourEvent::DropSession { session_id },
            });
        }
        Ok(())
    }

    fn get_peer_id_and_connection_id_from_session_id(
        &self,
        session_id: SessionId,
    ) -> Result<(PeerId, ConnectionId), SessionIdNotFoundError> {
        self.session_id_to_peer_id_and_connection_id
            .get(&session_id)
            .copied()
            .ok_or(SessionIdNotFoundError)
    }
}

impl<Query: QueryBound, Data: DataBound> NetworkBehaviour for Behaviour<Query, Data> {
    type ConnectionHandler = Handler<Query, Data>;
    type ToSwarm = Event<Query, Data>;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer_id: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler::new(self.config.clone(), self.next_inbound_session_id.clone(), peer_id))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer_id: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler::new(self.config.clone(), self.next_inbound_session_id.clone(), peer_id))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connection_ids_map.get_mut(peer_id).insert(connection_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, connection_id, .. }) => {
                self.session_id_to_peer_id_and_connection_id.retain(
                    |session_id, (session_peer_id, session_connection_id)| {
                        if peer_id == *session_peer_id && connection_id == *session_connection_id {
                            self.pending_events.push_back(ToSwarm::GenerateEvent(
                                Event::SessionFailed {
                                    session_id: *session_id,
                                    error: SessionError::ConnectionClosed,
                                },
                            ));
                            false
                        } else {
                            true
                        }
                    },
                );
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        match event {
            RequestToBehaviourEvent::GenerateEvent(event) => {
                let converted_event = event.into();
                let mut is_event_muted = false;
                match converted_event {
                    Event::NewInboundSession { inbound_session_id, .. } => {
                        self.session_id_to_peer_id_and_connection_id
                            .insert(inbound_session_id.into(), (peer_id, connection_id));
                    }
                    Event::SessionFailed { session_id, .. }
                    | Event::SessionFinishedSuccessfully { session_id, .. } => {
                        self.session_id_to_peer_id_and_connection_id.remove(&session_id);
                        let is_dropped = self.dropped_sessions.remove(&session_id);
                        if is_dropped {
                            is_event_muted = true;
                        }
                    }
                    Event::ReceivedData { outbound_session_id, .. } => {
                        if self.dropped_sessions.contains(&outbound_session_id.into()) {
                            is_event_muted = true;
                        }
                    }
                }
                if !is_event_muted {
                    self.pending_events.push_back(ToSwarm::GenerateEvent(converted_event));
                }
            }
            RequestToBehaviourEvent::NotifySessionDropped { session_id } => {
                self.dropped_sessions.remove(&session_id);
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        Poll::Pending
    }
}
