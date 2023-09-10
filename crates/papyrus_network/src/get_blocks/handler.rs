#[cfg(test)]
#[path = "handler_test.rs"]
mod handler_test;

use std::collections::{HashMap, VecDeque};
use std::io;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::channel::mpsc::{TrySendError, UnboundedReceiver};
use futures::StreamExt;
use libp2p::swarm::handler::{ConnectionEvent, DialUpgradeError, FullyNegotiatedOutbound};
use libp2p::swarm::{
    ConnectionHandler,
    ConnectionHandlerEvent,
    KeepAlive,
    StreamUpgradeError,
    SubstreamProtocol,
};
use prost::Message;

use super::protocol::{OutboundProtocol, OutboundProtocolError, ResponseProtocol, PROTOCOL_NAME};
use super::OutboundSessionId;

// TODO(shahak): Add a FromBehaviour event for cancelling an existing request.
#[derive(Debug)]
pub struct NewQueryEvent<Query: Message> {
    pub query: Query,
    pub outbound_session_id: OutboundSessionId,
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError<Data> {
    #[error("Connection timed out after {} seconds.", substream_timeout.as_secs())]
    Timeout { substream_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    ResponseSendError(#[from] TrySendError<Data>),
    #[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
    RemoteDoesntSupportProtocol,
}

#[derive(thiserror::Error, Debug)]
#[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
pub struct RemoteDoesntSupportProtocolError;

#[derive(Debug)]
pub enum SessionProgressEvent<Data: Message> {
    ReceivedData { outbound_session_id: OutboundSessionId, data: Data },
    SessionFinished { outbound_session_id: OutboundSessionId },
    SessionFailed { outbound_session_id: OutboundSessionId, error: RequestError<Data> },
}

type HandlerEvent<H> = ConnectionHandlerEvent<
    <H as ConnectionHandler>::OutboundProtocol,
    <H as ConnectionHandler>::OutboundOpenInfo,
    <H as ConnectionHandler>::ToBehaviour,
    <H as ConnectionHandler>::Error,
>;

pub struct Handler<Query: Message + 'static, Data: Message + 'static + Default> {
    substream_timeout: Duration,
    outbound_session_id_to_data_receiver: HashMap<OutboundSessionId, UnboundedReceiver<Data>>,
    pending_events: VecDeque<HandlerEvent<Self>>,
    ready_outbound_data: VecDeque<(OutboundSessionId, Data)>,
}

impl<Query: Message + 'static, Data: Message + 'static + Default> Handler<Query, Data> {
    // TODO(shahak) If we'll add more parameters, consider creating a HandlerConfig struct.
    pub fn new(substream_timeout: Duration) -> Self {
        Self {
            substream_timeout,
            outbound_session_id_to_data_receiver: Default::default(),
            pending_events: Default::default(),
            ready_outbound_data: Default::default(),
        }
    }

    fn convert_upgrade_error(
        &self,
        error: StreamUpgradeError<OutboundProtocolError<Data>>,
    ) -> RequestError<Data> {
        match error {
            StreamUpgradeError::Timeout => {
                RequestError::Timeout { substream_timeout: self.substream_timeout }
            }
            StreamUpgradeError::Apply(request_protocol_error) => match request_protocol_error {
                OutboundProtocolError::IOError(error) => RequestError::IOError(error),
                OutboundProtocolError::ResponseSendError(error) => {
                    RequestError::ResponseSendError(error)
                }
            },
            StreamUpgradeError::NegotiationFailed => RequestError::RemoteDoesntSupportProtocol,
            StreamUpgradeError::Io(error) => RequestError::IOError(error),
        }
    }

    fn clear_pending_events_related_to_session(&mut self, outbound_session_id: OutboundSessionId) {
        self.pending_events.retain(|event| match event {
            ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::ReceivedData {
                outbound_session_id: other_outbound_session_id,
                ..
            }) => outbound_session_id != *other_outbound_session_id,
            _ => true,
        })
    }
}

impl<Query: Message + 'static, Data: Message + 'static + Default> ConnectionHandler
    for Handler<Query, Data>
{
    type FromBehaviour = NewQueryEvent<Query>;
    type ToBehaviour = SessionProgressEvent<Data>;
    type Error = RemoteDoesntSupportProtocolError;
    type InboundProtocol = ResponseProtocol;
    type OutboundProtocol = OutboundProtocol<Query, Data>;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = OutboundSessionId;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(ResponseProtocol {}, ()).with_timeout(self.substream_timeout)
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
        // TODO(shahak): Consider handling incoming messages interleaved with handling pending
        // events to avoid starvation.
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }

        // Handle incoming messages.
        for (request_id, responses_receiver) in &mut self.outbound_session_id_to_data_receiver {
            if let Poll::Ready(Some(response)) = responses_receiver.poll_next_unpin(cx) {
                // Collect all ready responses to avoid starvation of the request ids at the end.
                self.ready_outbound_data.push_back((*request_id, response));
            }
        }
        if let Some((outbound_session_id, data)) = self.ready_outbound_data.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::<
                Data,
            >::ReceivedData {
                outbound_session_id,
                data,
            }));
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        // There's only one type of event so we can unpack it without matching.
        let NewQueryEvent { query, outbound_session_id } = event;
        let (request_protocol, responses_receiver) = OutboundProtocol::new(query);
        let insert_result = self
            .outbound_session_id_to_data_receiver
            .insert(outbound_session_id, responses_receiver);
        if insert_result.is_some() {
            panic!("Multiple requests exist with the same ID {}", outbound_session_id);
        }
        self.pending_events.push_back(ConnectionHandlerEvent::OutboundSubstreamRequest {
            protocol: SubstreamProtocol::new(request_protocol, outbound_session_id)
                .with_timeout(self.substream_timeout),
        });
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
                protocol: _,
                info: outbound_session_id,
            }) => {
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    SessionProgressEvent::SessionFinished { outbound_session_id },
                ));
                self.outbound_session_id_to_data_receiver.remove(&outbound_session_id);
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError {
                info: outbound_session_id,
                error,
            }) => {
                let error = self.convert_upgrade_error(error);
                if matches!(error, RequestError::RemoteDoesntSupportProtocol) {
                    // This error will happen on all future connections to the peer, so we'll close
                    // the handle after reporting to the behaviour.
                    self.pending_events.clear();
                    self.pending_events.push_front(ConnectionHandlerEvent::NotifyBehaviour(
                        SessionProgressEvent::SessionFailed { outbound_session_id, error },
                    ));
                    self.pending_events
                        .push_back(ConnectionHandlerEvent::Close(RemoteDoesntSupportProtocolError));
                } else {
                    self.clear_pending_events_related_to_session(outbound_session_id);
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        SessionProgressEvent::SessionFailed { outbound_session_id, error },
                    ));
                }
                self.outbound_session_id_to_data_receiver.remove(&outbound_session_id);
            }
            ConnectionEvent::FullyNegotiatedInbound(_)
            | ConnectionEvent::ListenUpgradeError(_)
            | ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
        }
    }
}
