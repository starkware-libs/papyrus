#[cfg(test)]
#[path = "handler_test.rs"]
mod handler_test;
mod session;

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
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, KeepAlive, SubstreamProtocol};
use tracing::debug;

use self::session::{FinishReason, InboundSession};
use super::protocol::{InboundProtocol, OutboundProtocol, PROTOCOL_NAME};
use super::{DataBound, GenericEvent, InboundSessionId, OutboundSessionId, QueryBound, SessionId};
use crate::messages::read_message;

#[derive(Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum RequestFromBehaviourEvent<Query, Data> {
    CreateOutboundSession { query: Query, outbound_session_id: OutboundSessionId },
    SendData { data: Data, inbound_session_id: InboundSessionId },
    CloseSession { session_id: SessionId },
}

#[derive(thiserror::Error, Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum SessionError {
    #[error("Connection timed out after {} seconds.", substream_timeout.as_secs())]
    Timeout { substream_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
}

pub(crate) type ToBehaviourEvent<Query, Data> = GenericEvent<Query, Data, SessionError>;

#[derive(thiserror::Error, Debug)]
#[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
pub(crate) struct RemoteDoesntSupportProtocolError;

type HandlerEvent<H> = ConnectionHandlerEvent<
    <H as ConnectionHandler>::OutboundProtocol,
    <H as ConnectionHandler>::OutboundOpenInfo,
    <H as ConnectionHandler>::ToBehaviour,
    <H as ConnectionHandler>::Error,
>;

pub(crate) struct Handler<Query: QueryBound, Data: DataBound> {
    substream_timeout: Duration,
    next_inbound_session_id: Arc<AtomicUsize>,
    id_to_inbound_session: HashMap<InboundSessionId, InboundSession<Data>>,
    id_to_outbound_session: HashMap<OutboundSessionId, BoxStream<'static, Result<Data, io::Error>>>,
    pending_events: VecDeque<HandlerEvent<Self>>,
    inbound_sessions_marked_to_end: HashSet<InboundSessionId>,
}

impl<Query: QueryBound, Data: DataBound> Handler<Query, Data> {
    // TODO(shahak) If we'll add more parameters, consider creating a HandlerConfig struct.
    // TODO(shahak) remove allow(dead_code).
    #[allow(dead_code)]
    pub fn new(substream_timeout: Duration, next_inbound_session_id: Arc<AtomicUsize>) -> Self {
        Self {
            substream_timeout,
            next_inbound_session_id,
            id_to_inbound_session: Default::default(),
            id_to_outbound_session: Default::default(),
            pending_events: Default::default(),
            inbound_sessions_marked_to_end: Default::default(),
        }
    }

    // fn convert_upgrade_error(
    //     &self,
    //     error: StreamUpgradeError<OutboundProtocolError<Data>>,
    // ) -> RequestError<Data> { match error { StreamUpgradeError::Timeout => {
    //   RequestError::Timeout { substream_timeout: self.substream_timeout } }
    //   StreamUpgradeError::Apply(request_protocol_error) => match request_protocol_error {
    //   OutboundProtocolError::IOError(error) => RequestError::IOError(error),
    //   OutboundProtocolError::ResponseSendError(error) => { RequestError::ResponseSendError(error)
    //   } }, StreamUpgradeError::NegotiationFailed => RequestError::RemoteDoesntSupportProtocol,
    //   StreamUpgradeError::Io(error) => RequestError::IOError(error), }
    // }

    // fn clear_pending_events_related_to_session(&mut self, outbound_session_id: OutboundSessionId)
    // {     self.pending_events.retain(|event| match event {
    //         ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::ReceivedData {
    //             outbound_session_id: other_outbound_session_id,
    //             ..
    //         }) => outbound_session_id != *other_outbound_session_id,
    //         _ => true,
    //     })
    // }

    /// Poll an inbound session, inserting any events needed to pending_events, and return whether
    /// the inbound session has finished.
    fn poll_inbound_session(
        inbound_session: &mut InboundSession<Data>,
        inbound_session_id: InboundSessionId,
        pending_events: &mut VecDeque<HandlerEvent<Self>>,
        cx: &mut Context<'_>,
    ) -> bool {
        let Poll::Ready(finish_reason) = inbound_session.poll_unpin(cx) else {
            let is_session_alive = false;
            return is_session_alive;
        };
        if let FinishReason::Error(io_error) = finish_reason {
            pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                ToBehaviourEvent::SessionFailed {
                    session_id: SessionId::InboundSessionId(inbound_session_id),
                    error: SessionError::IOError(io_error),
                },
            ));
        }
        true
    }
}

impl<Query: QueryBound, Data: DataBound> ConnectionHandler for Handler<Query, Data> {
    type FromBehaviour = RequestFromBehaviourEvent<Query, Data>;
    type ToBehaviour = ToBehaviourEvent<Query, Data>;
    type Error = RemoteDoesntSupportProtocolError;
    type InboundProtocol = InboundProtocol<Query>;
    type OutboundProtocol = OutboundProtocol<Query>;
    type InboundOpenInfo = InboundSessionId;
    type OutboundOpenInfo = OutboundSessionId;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(
            InboundProtocol::new(),
            InboundSessionId { value: self.next_inbound_session_id.fetch_add(1, Ordering::AcqRel) },
        )
        .with_timeout(self.substream_timeout)
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        // TODO(shahak): Implement keep alive logic.
        KeepAlive::Yes
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::ToBehaviour,
            Self::Error,
        >,
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
                        ToBehaviourEvent::ReceivedData {
                            outbound_session_id: *outbound_session_id,
                            data,
                        },
                    ));
                    true
                }
                Poll::Ready(Some(Err(io_error))) => {
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        ToBehaviourEvent::SessionFailed {
                            session_id: SessionId::OutboundSessionId(*outbound_session_id),
                            error: SessionError::IOError(io_error),
                        },
                    ));
                    false
                }
                Poll::Ready(None) => {
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        ToBehaviourEvent::OutboundSessionClosedByPeer {
                            outbound_session_id: *outbound_session_id,
                        },
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
                        OutboundProtocol { query },
                        outbound_session_id,
                    )
                    .with_timeout(self.substream_timeout),
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
            RequestFromBehaviourEvent::CloseSession {
                session_id: SessionId::InboundSessionId(inbound_session_id),
            } => {
                self.inbound_sessions_marked_to_end.insert(inbound_session_id);
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    ToBehaviourEvent::SessionClosedByRequest {
                        session_id: SessionId::InboundSessionId(inbound_session_id),
                    },
                ));
            }
            RequestFromBehaviourEvent::CloseSession {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
            } => {
                self.id_to_outbound_session.remove(&outbound_session_id);
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    ToBehaviourEvent::SessionClosedByRequest {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
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
                protocol: mut stream,
                info: outbound_session_id,
            }) => {
                self.id_to_outbound_session.insert(
                    outbound_session_id,
                    stream! {
                        loop {
                            let result_opt = read_message::<Data, _>(&mut stream).await;
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
                protocol: (query, stream),
                info: inbound_session_id,
            }) => {
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    ToBehaviourEvent::NewInboundSession { query, inbound_session_id },
                ));
                self.id_to_inbound_session.insert(inbound_session_id, InboundSession::new(stream));
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError {
                info: _outbound_session_id,
                error: _error,
            }) => {
                unimplemented!();
                // let error = self.convert_upgrade_error(error);
                // if matches!(error, RequestError::RemoteDoesntSupportProtocol) {
                //     // This error will happen on all future connections to the peer, so we'll
                //     // close the handle after reporting to the behaviour.
                //     self.pending_events.clear();
                //     self.pending_events.push_front(ConnectionHandlerEvent::NotifyBehaviour(
                //         SessionProgressEvent::SessionFailed { outbound_session_id, error },
                //     ));
                //     self.pending_events
                //         .push_back(ConnectionHandlerEvent::Close(RemoteDoesntSupportProtocolError));
                // } else {
                //     self.clear_pending_events_related_to_session(outbound_session_id);
                //     self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                //         SessionProgressEvent::SessionFailed { outbound_session_id, error },
                //     ));
                // }
                // self.outbound_session_id_to_data_receiver.remove(&outbound_session_id);
            }
            ConnectionEvent::ListenUpgradeError(_)
            | ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
        }
    }
}
