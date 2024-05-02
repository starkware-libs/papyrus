#[cfg(test)]
#[path = "behaviour_test.rs"]
mod behaviour_test;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use libp2p::core::Endpoint;
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
use tracing::{error, info};

use super::handler::{
    Handler,
    RequestFromBehaviourEvent,
    RequestToBehaviourEvent,
    SessionError as HandlerSessionError,
};
use super::{Bytes, Config, GenericEvent, InboundSessionId, OutboundSessionId, SessionId};
use crate::main_behaviour::mixed_behaviour::{self, BridgedBehaviour};
use crate::peer_manager;

#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error("Connection timed out after {} seconds.", session_timeout.as_secs())]
    Timeout { session_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Remote peer doesn't support the given protocol.")]
    RemoteDoesntSupportProtocol,
    #[error("In an inbound session, remote peer sent data after sending the query.")]
    OtherOutboundPeerSentData,
    // If there's a connection with a single session and it was closed because of another reason,
    // we might get ConnectionClosed instead of that reason because the swarm automatically closes
    // a connection that has no sessions. If this is a problem, set the swarm's
    // idle_connection_timeout to a non-zero number.
    #[error("Connection to remote peer closed.")]
    ConnectionClosed,
}

impl From<GenericEvent<HandlerSessionError>> for GenericEvent<SessionError> {
    fn from(event: GenericEvent<HandlerSessionError>) -> Self {
        match event {
            GenericEvent::NewInboundSession {
                query,
                inbound_session_id,
                peer_id,
                protocol_name,
            } => Self::NewInboundSession { query, inbound_session_id, peer_id, protocol_name },
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
                error: HandlerSessionError::RemoteDoesntSupportProtocol,
            } => {
                Self::SessionFailed { session_id, error: SessionError::RemoteDoesntSupportProtocol }
            }
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

pub type ExternalEvent = GenericEvent<SessionError>;

#[derive(Debug)]
pub enum ToOtherBehaviour {
    NotifyPeerManager(peer_manager::FromOtherBehaviour),
}

#[derive(Debug)]
pub enum Event {
    External(ExternalEvent),
    ToOtherBehaviour(ToOtherBehaviour),
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum FromOtherBehaviour {
    SessionAssigned {
        outbound_session_id: OutboundSessionId,
        peer_id: PeerId,
        connection_id: ConnectionId,
    },
}

#[derive(thiserror::Error, Debug)]
#[error("The given session ID doesn't exist.")]
pub struct SessionIdNotFoundError;

#[derive(thiserror::Error, Debug)]
#[error("We are not connected to the given peer. Dial to the given peer and try again.")]
pub struct PeerNotConnected;

pub struct Behaviour {
    config: Config,
    pending_events: VecDeque<ToSwarm<Event, RequestFromBehaviourEvent>>,
    session_id_to_peer_id_and_connection_id: HashMap<SessionId, (PeerId, ConnectionId)>,
    next_outbound_session_id: OutboundSessionId,
    next_inbound_session_id: Arc<AtomicUsize>,
    dropped_sessions: HashSet<SessionId>,
    wakers_waiting_for_event: Vec<Waker>,
    outbound_sessions_pending_peer_assignment: HashMap<OutboundSessionId, (Bytes, StreamProtocol)>,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            pending_events: Default::default(),
            session_id_to_peer_id_and_connection_id: Default::default(),
            next_outbound_session_id: Default::default(),
            next_inbound_session_id: Arc::new(Default::default()),
            dropped_sessions: Default::default(),
            wakers_waiting_for_event: Default::default(),
            outbound_sessions_pending_peer_assignment: Default::default(),
        }
    }

    /// Assign some peer and start a query. Return the id of the new session.
    pub fn start_query(
        &mut self,
        query: Bytes,
        protocol_name: StreamProtocol,
    ) -> OutboundSessionId {
        let outbound_session_id = self.next_outbound_session_id;
        self.next_outbound_session_id.value += 1;

        self.outbound_sessions_pending_peer_assignment
            .insert(outbound_session_id, (query, protocol_name));
        info!("Requesting peer assignment for outbound session: {:?}.", outbound_session_id);
        self.add_event_to_queue(ToSwarm::GenerateEvent(Event::ToOtherBehaviour(
            ToOtherBehaviour::NotifyPeerManager(
                peer_manager::FromOtherBehaviour::RequestPeerAssignment { outbound_session_id },
            ),
        )));

        outbound_session_id
    }

    /// Send a data message to an open inbound session.
    pub fn send_length_prefixed_data(
        &mut self,
        data: Bytes,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let (peer_id, connection_id) =
            self.get_peer_id_and_connection_id_from_session_id(inbound_session_id.into())?;
        self.add_event_to_queue(ToSwarm::NotifyHandler {
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
        self.add_event_to_queue(ToSwarm::NotifyHandler {
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
            self.add_event_to_queue(ToSwarm::NotifyHandler {
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

    fn add_event_to_queue(&mut self, event: ToSwarm<Event, RequestFromBehaviourEvent>) {
        self.pending_events.push_back(event);
        for waker in self.wakers_waiting_for_event.drain(..) {
            waker.wake();
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

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
            FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, connection_id, .. }) => {
                let mut session_ids = Vec::new();
                self.session_id_to_peer_id_and_connection_id.retain(
                    |session_id, (session_peer_id, session_connection_id)| {
                        if peer_id == *session_peer_id && connection_id == *session_connection_id {
                            session_ids.push(*session_id);
                            false
                        } else {
                            true
                        }
                    },
                );
                for session_id in session_ids {
                    self.add_event_to_queue(ToSwarm::GenerateEvent(Event::External(
                        ExternalEvent::SessionFailed {
                            session_id,
                            error: SessionError::ConnectionClosed,
                        },
                    )));
                }
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
                    ExternalEvent::NewInboundSession { inbound_session_id, .. } => {
                        self.session_id_to_peer_id_and_connection_id
                            .insert(inbound_session_id.into(), (peer_id, connection_id));
                    }
                    ExternalEvent::SessionFailed { session_id, .. }
                    | ExternalEvent::SessionFinishedSuccessfully { session_id, .. } => {
                        self.session_id_to_peer_id_and_connection_id.remove(&session_id);
                        let is_dropped = self.dropped_sessions.remove(&session_id);
                        if is_dropped {
                            is_event_muted = true;
                        }
                    }
                    ExternalEvent::ReceivedData { outbound_session_id, .. } => {
                        if self.dropped_sessions.contains(&outbound_session_id.into()) {
                            is_event_muted = true;
                        }
                    }
                }
                if !is_event_muted {
                    self.add_event_to_queue(ToSwarm::GenerateEvent(Event::External(
                        converted_event,
                    )));
                }
            }
            RequestToBehaviourEvent::NotifySessionDropped { session_id } => {
                self.dropped_sessions.remove(&session_id);
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        self.wakers_waiting_for_event.push(cx.waker().clone());
        Poll::Pending
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, event: mixed_behaviour::InternalEvent) {
        let mixed_behaviour::InternalEvent::NotifyStreamedBytes(event) = event else {
            return;
        };
        match event {
            FromOtherBehaviour::SessionAssigned { outbound_session_id, peer_id, connection_id } => {
                self.session_id_to_peer_id_and_connection_id
                    .insert(outbound_session_id.into(), (peer_id, connection_id));

                let Some((query, protocol_name)) =
                    self.outbound_sessions_pending_peer_assignment.remove(&outbound_session_id)
                else {
                    error!(
                        "Outbound session assigned peer but it isn't in \
                         outbound_sessions_pending_peer_assignment. Not running query."
                    );
                    return;
                };

                self.add_event_to_queue(ToSwarm::NotifyHandler {
                    peer_id,
                    handler: NotifyHandler::One(connection_id),
                    event: RequestFromBehaviourEvent::CreateOutboundSession {
                        query,
                        outbound_session_id,
                        protocol_name,
                    },
                });
            }
        }
    }
}

impl From<Event> for mixed_behaviour::Event {
    fn from(event: Event) -> Self {
        match event {
            Event::External(external_event) => {
                Self::ExternalEvent(mixed_behaviour::ExternalEvent::StreamedBytes(external_event))
            }
            Event::ToOtherBehaviour(ToOtherBehaviour::NotifyPeerManager(event)) => {
                Self::InternalEvent(mixed_behaviour::InternalEvent::NotifyPeerManager(event))
            }
        }
    }
}
