#[cfg(test)]
#[path = "handler_test.rs"]
mod handler_test;
mod inbound_session;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_stream::stream;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use libp2p::swarm::handler::{
    ConnectionEvent,
    DialUpgradeError,
    FullyNegotiatedInbound,
    FullyNegotiatedOutbound,
};
use libp2p::swarm::{
    ConnectionHandler,
    ConnectionHandlerEvent,
    StreamProtocol,
    StreamUpgradeError,
    SubstreamProtocol,
};
use libp2p::PeerId;
use tracing::debug;

use self::inbound_session::InboundSession;
use super::protocol::{InboundProtocol, OutboundProtocol};
use super::{
    Config,
    DataBound,
    GenericEvent,
    InboundSessionId,
    OutboundSessionId,
    QueryBound,
    SessionId,
};
use crate::messages::read_message;

#[derive(Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub enum RequestFromBehaviourEvent<Query, Data> {
    CreateOutboundSession { query: Query, outbound_session_id: OutboundSessionId },
    SendData { data: Data, inbound_session_id: InboundSessionId },
    CloseInboundSession { inbound_session_id: InboundSessionId },
    DropSession { session_id: SessionId },
}

#[derive(Debug)]
pub enum RequestToBehaviourEvent<Query: QueryBound, Data: DataBound> {
    GenerateEvent(GenericEvent<Query, Data, SessionError>),
    NotifySessionDropped { session_id: SessionId },
}

#[derive(thiserror::Error, Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub enum SessionError {
    #[error("Connection timed out after {} seconds.", session_timeout.as_secs())]
    Timeout { session_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Remote peer doesn't support the {protocol_name} protocol.")]
    RemoteDoesntSupportProtocol { protocol_name: StreamProtocol },
    // TODO(shahak) erase this.
    #[error("In an inbound session, remote peer sent data after sending the query.")]
    OtherOutboundPeerSentData,
}

type HandlerEvent<H> = ConnectionHandlerEvent<
    <H as ConnectionHandler>::OutboundProtocol,
    <H as ConnectionHandler>::OutboundOpenInfo,
    <H as ConnectionHandler>::ToBehaviour,
>;

pub struct Handler<Query: QueryBound, Data: DataBound> {
    // TODO(shahak): Consider changing to Arc<Config> if the config becomes heavy to clone.
    config: Config,
    next_inbound_session_id: Arc<AtomicUsize>,
    peer_id: PeerId,
    id_to_inbound_session: HashMap<InboundSessionId, InboundSession<Data>>,
    id_to_outbound_session: HashMap<OutboundSessionId, BoxStream<'static, Result<Data, io::Error>>>,
    // TODO(shahak): Use deadqueue if using a VecDeque is a bug (libp2p uses VecDeque, so we opened
    // an issue on it https://github.com/libp2p/rust-libp2p/issues/5147)
    pending_events: VecDeque<HandlerEvent<Self>>,
    inbound_sessions_marked_to_end: HashSet<InboundSessionId>,
    dropped_outbound_sessions_non_negotiated: HashSet<OutboundSessionId>,
}

impl<Query: QueryBound, Data: DataBound> Handler<Query, Data> {
    // TODO(shahak) If we'll add more parameters, consider creating a HandlerConfig struct.
    // TODO(shahak) remove allow(dead_code).
    #[allow(dead_code)]
    pub fn new(config: Config, next_inbound_session_id: Arc<AtomicUsize>, peer_id: PeerId) -> Self {
        Self {
            config,
            next_inbound_session_id,
            peer_id,
            id_to_inbound_session: Default::default(),
            id_to_outbound_session: Default::default(),
            pending_events: Default::default(),
            inbound_sessions_marked_to_end: Default::default(),
            dropped_outbound_sessions_non_negotiated: Default::default(),
        }
    }

    /// Poll an inbound session, inserting any events needed to pending_events, and return whether
    /// the inbound session has finished.
    fn poll_inbound_session(
        inbound_session: &mut InboundSession<Data>,
        inbound_session_id: InboundSessionId,
        pending_events: &mut VecDeque<HandlerEvent<Self>>,
        cx: &mut Context<'_>,
    ) -> bool {
        match inbound_session.poll_unpin(cx) {
            Poll::Ready(Err(io_error)) => {
                pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFailed {
                        session_id: inbound_session_id.into(),
                        error: SessionError::IOError(io_error),
                    }),
                ));
                true
            }
            Poll::Ready(Ok(())) => {
                pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestToBehaviourEvent::GenerateEvent(
                        GenericEvent::SessionFinishedSuccessfully {
                            session_id: inbound_session_id.into(),
                        },
                    ),
                ));
                true
            }
            Poll::Pending => false,
        }
    }
}

impl<Query: QueryBound, Data: DataBound> ConnectionHandler for Handler<Query, Data> {
    type FromBehaviour = RequestFromBehaviourEvent<Query, Data>;
    type ToBehaviour = RequestToBehaviourEvent<Query, Data>;
    type InboundProtocol = InboundProtocol<Query>;
    type OutboundProtocol = OutboundProtocol<Query>;
    type InboundOpenInfo = InboundSessionId;
    type OutboundOpenInfo = OutboundSessionId;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(
            InboundProtocol::new(self.config.protocol_name.clone()),
            InboundSessionId { value: self.next_inbound_session_id.fetch_add(1, Ordering::AcqRel) },
        )
        .with_timeout(self.config.session_timeout)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        // Handle inbound sessions.
        self.id_to_inbound_session.retain(|inbound_session_id, inbound_session| {
            if Self::poll_inbound_session(
                inbound_session,
                *inbound_session_id,
                &mut self.pending_events,
                cx,
            ) {
                let is_session_alive = false;
                return is_session_alive;
            }
            if self.inbound_sessions_marked_to_end.contains(inbound_session_id)
                && inbound_session.is_waiting()
            {
                inbound_session.start_closing();
                if Self::poll_inbound_session(
                    inbound_session,
                    *inbound_session_id,
                    &mut self.pending_events,
                    cx,
                ) {
                    let is_session_alive = false;
                    return is_session_alive;
                }
            }
            true
        });

        // Handle outbound sessions.
        self.id_to_outbound_session.retain(|outbound_session_id, outbound_session| {
            match outbound_session.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(data))) => {
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        RequestToBehaviourEvent::GenerateEvent(GenericEvent::ReceivedData {
                            outbound_session_id: *outbound_session_id,
                            data,
                        }),
                    ));
                    true
                }
                Poll::Ready(Some(Err(io_error))) => {
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFailed {
                            session_id: SessionId::OutboundSessionId(*outbound_session_id),
                            error: SessionError::IOError(io_error),
                        }),
                    ));
                    false
                }
                Poll::Ready(None) => {
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        RequestToBehaviourEvent::GenerateEvent(
                            GenericEvent::SessionFinishedSuccessfully {
                                session_id: SessionId::OutboundSessionId(*outbound_session_id),
                            },
                        ),
                    ));
                    false
                }
                Poll::Pending => true,
            }
        });

        // Handling pending_events at the end of the function to avoid starvation.
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            RequestFromBehaviourEvent::CreateOutboundSession { query, outbound_session_id } => {
                // TODO(shahak) Consider extracting to a utility function to prevent forgetfulness
                // of the timeout.
                self.pending_events.push_back(ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(
                        OutboundProtocol {
                            query,
                            protocol_name: self.config.protocol_name.clone(),
                        },
                        outbound_session_id,
                    )
                    .with_timeout(self.config.session_timeout),
                });
            }
            RequestFromBehaviourEvent::SendData { data, inbound_session_id } => {
                if let Some(inbound_session) =
                    self.id_to_inbound_session.get_mut(&inbound_session_id)
                {
                    if self.inbound_sessions_marked_to_end.contains(&inbound_session_id) {
                        // TODO(shahak): Consider handling this in a different way than just
                        // logging.
                        debug!(
                            "Got a request to send data on a closed inbound session with id \
                             {inbound_session_id}. Ignoring request."
                        );
                    } else {
                        inbound_session.add_message_to_queue(data);
                    }
                } else {
                    // TODO(shahak): Consider handling this in a different way than just logging.
                    debug!(
                        "Got a request to send data on a non-existing or closed inbound session \
                         with id {inbound_session_id}. Ignoring request."
                    );
                }
            }
            RequestFromBehaviourEvent::CloseInboundSession { inbound_session_id } => {
                self.inbound_sessions_marked_to_end.insert(inbound_session_id);
            }
            RequestFromBehaviourEvent::DropSession {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
            } => {
                let remove_result = self.id_to_outbound_session.remove(&outbound_session_id);
                if remove_result.is_none() {
                    self.dropped_outbound_sessions_non_negotiated.insert(outbound_session_id);
                }
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestToBehaviourEvent::NotifySessionDropped {
                        session_id: outbound_session_id.into(),
                    },
                ));
            }
            RequestFromBehaviourEvent::DropSession {
                session_id: SessionId::InboundSessionId(inbound_session_id),
            } => {
                self.id_to_inbound_session.remove(&inbound_session_id);
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestToBehaviourEvent::NotifySessionDropped {
                        session_id: inbound_session_id.into(),
                    },
                ));
            }
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            '_,
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: mut read_stream,
                info: outbound_session_id,
            }) => {
                if self.dropped_outbound_sessions_non_negotiated.remove(&outbound_session_id) {
                    return;
                }
                self.id_to_outbound_session.insert(
                    outbound_session_id,
                    stream! {
                        loop {
                            let result_opt = read_message::<Data, _>(&mut read_stream).await;
                            let result = match result_opt {
                                Ok(Some(data)) => Ok(data),
                                Ok(None) => break,
                                Err(error) => Err(error),
                            };
                            let is_err = result.is_err();
                            yield result;
                            if is_err {
                                break;
                            }
                        }
                    }
                    .boxed(),
                );
            }
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: (query, write_stream),
                info: inbound_session_id,
            }) => {
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestToBehaviourEvent::GenerateEvent(GenericEvent::NewInboundSession {
                        query,
                        inbound_session_id,
                        peer_id: self.peer_id,
                    }),
                ));
                self.id_to_inbound_session
                    .insert(inbound_session_id, InboundSession::new(write_stream));
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError {
                info: outbound_session_id,
                error: upgrade_error,
            }) => {
                let session_error = match upgrade_error {
                    StreamUpgradeError::Timeout => {
                        SessionError::Timeout { session_timeout: self.config.session_timeout }
                    }
                    StreamUpgradeError::Apply(outbound_protocol_error) => {
                        SessionError::IOError(outbound_protocol_error)
                    }
                    StreamUpgradeError::NegotiationFailed => {
                        SessionError::RemoteDoesntSupportProtocol {
                            protocol_name: self.config.protocol_name.clone(),
                        }
                    }
                    StreamUpgradeError::Io(error) => SessionError::IOError(error),
                };
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFailed {
                        session_id: outbound_session_id.into(),
                        error: session_error,
                    }),
                ));
            }
            // We don't need to handle a ListenUpgradeError because an inbound session is created
            // only after a successful upgrade so there's no session failure to report.
            _ => {}
        }
    }
}
