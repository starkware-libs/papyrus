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
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    NotifyHandler,
    PollParameters,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use super::handler::{Handler, RequestFromBehaviourEvent, SessionError as HandlerSessionError};
use super::protocol::PROTOCOL_NAME;
use super::{DataBound, GenericEvent, InboundSessionId, OutboundSessionId, QueryBound, SessionId};

#[derive(thiserror::Error, Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum SessionError {
    #[error("Connection timed out after {} seconds.", substream_timeout.as_secs())]
    Timeout { substream_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
    // TODO(shahak) make PROTOCOL_NAME configurable.
    #[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
    RemoteDoesntSupportProtocol,
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
                error: HandlerSessionError::Timeout { substream_timeout },
            } => Self::SessionFailed {
                session_id,
                error: SessionError::Timeout { substream_timeout },
            },
            GenericEvent::SessionFailed {
                session_id,
                error: HandlerSessionError::IOError(error),
            } => Self::SessionFailed { session_id, error: SessionError::IOError(error) },
            GenericEvent::SessionClosedByRequest { session_id } => {
                Self::SessionClosedByRequest { session_id }
            }
            GenericEvent::OutboundSessionClosedByPeer { outbound_session_id } => {
                Self::OutboundSessionClosedByPeer { outbound_session_id }
            }
        }
    }
}

pub(crate) type Event<Query, Data> = GenericEvent<Query, Data, SessionError>;

#[derive(thiserror::Error, Debug)]
#[error("The given session ID doesn't exist.")]
pub(crate) struct SessionIdNotFoundError;

#[derive(thiserror::Error, Debug)]
#[error("We are not connected to the given peer. Dial to the given peer and try again.")]
pub(crate) struct PeerNotConnected;

// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
pub(crate) struct Behaviour<Query: QueryBound, Data: DataBound> {
    substream_timeout: Duration,
    pending_events: VecDeque<ToSwarm<Event<Query, Data>, RequestFromBehaviourEvent<Query, Data>>>,
    pending_queries: DefaultHashMap<PeerId, Vec<(Query, OutboundSessionId)>>,
    connection_ids_map: DefaultHashMap<PeerId, HashSet<ConnectionId>>,
    session_id_to_peer_id_and_connection_id: HashMap<SessionId, (PeerId, ConnectionId)>,
    next_outbound_session_id: OutboundSessionId,
    next_inbound_session_id: Arc<AtomicUsize>,
}

// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
impl<Query: QueryBound, Data: DataBound> Behaviour<Query, Data> {
    pub fn new(substream_timeout: Duration) -> Self {
        Self {
            substream_timeout,
            pending_events: Default::default(),
            pending_queries: Default::default(),
            connection_ids_map: Default::default(),
            session_id_to_peer_id_and_connection_id: Default::default(),
            next_outbound_session_id: Default::default(),
            next_inbound_session_id: Arc::new(Default::default()),
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
            .insert(SessionId::OutboundSessionId(outbound_session_id), (peer_id, connection_id));

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
        let (peer_id, connection_id) = self.get_peer_id_and_connection_id_from_session_id(
            SessionId::InboundSessionId(inbound_session_id),
        )?;
        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(connection_id),
            event: RequestFromBehaviourEvent::SendData { data, inbound_session_id },
        });
        Ok(())
    }

    /// Instruct behaviour to close session. A corresponding SessionClosedByRequest event will be
    /// reported when the session is closed.
    pub fn close_session(&mut self, session_id: SessionId) -> Result<(), SessionIdNotFoundError> {
        let (peer_id, connection_id) =
            self.get_peer_id_and_connection_id_from_session_id(session_id)?;
        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(connection_id),
            event: RequestFromBehaviourEvent::CloseSession { session_id },
        });
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
        Ok(Handler::new(self.substream_timeout, self.next_inbound_session_id.clone(), peer_id))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer_id: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler::new(self.substream_timeout, self.next_inbound_session_id.clone(), peer_id))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_, Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connection_ids_map.get_mut(peer_id).insert(connection_id);
            }
            FromSwarm::NewListener(_)
            | FromSwarm::NewListenAddr(_)
            | FromSwarm::ExpiredListenAddr(_)
            | FromSwarm::ListenerClosed(_)
            | FromSwarm::NewExternalAddrCandidate(_)
            | FromSwarm::ExternalAddrConfirmed(_)
            | FromSwarm::ExternalAddrExpired(_) => {}
            _ => {
                unimplemented!();
            }
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        let converted_event = event.into();
        if let Event::NewInboundSession { inbound_session_id, .. } = converted_event {
            self.session_id_to_peer_id_and_connection_id
                .insert(SessionId::InboundSessionId(inbound_session_id), (peer_id, connection_id));
        }
        self.pending_events.push_back(ToSwarm::GenerateEvent(converted_event));
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
        _params: &mut impl PollParameters,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        Poll::Pending
    }
}
