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

use super::protocol::{InboundProtocol, OutboundProtocol, OutboundProtocolError, PROTOCOL_NAME};
use super::RequestId;
use crate::messages::block::{GetBlocks, GetBlocksResponse};

// TODO(shahak): Add a FromBehaviour event for cancelling an existing request.
#[derive(Debug)]
pub struct NewRequestEvent {
    pub request: GetBlocks,
    pub request_id: RequestId,
}

#[derive(thiserror::Error, Debug)]
pub enum RequestError {
    #[error("Connection timed out after {} seconds.", substream_timeout.as_secs())]
    Timeout { substream_timeout: Duration },
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    ResponseSendError(#[from] TrySendError<GetBlocksResponse>),
    #[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
    RemoteDoesntSupportProtocol,
}

#[derive(thiserror::Error, Debug)]
#[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
pub struct RemoteDoesntSupportProtocolError;

#[derive(Debug)]
pub enum RequestProgressEvent {
    ReceivedResponse { request_id: RequestId, response: GetBlocksResponse },
    RequestFinished { request_id: RequestId },
    RequestFailed { request_id: RequestId, error: RequestError },
}

type HandlerEvent<H> = ConnectionHandlerEvent<
    <H as ConnectionHandler>::OutboundProtocol,
    <H as ConnectionHandler>::OutboundOpenInfo,
    <H as ConnectionHandler>::ToBehaviour,
    <H as ConnectionHandler>::Error,
>;

pub struct Handler {
    substream_timeout: Duration,
    request_to_responses_receiver: HashMap<RequestId, UnboundedReceiver<GetBlocksResponse>>,
    pending_events: VecDeque<HandlerEvent<Self>>,
    ready_requests: VecDeque<(RequestId, GetBlocksResponse)>,
}

impl Handler {
    // TODO(shahak) If we'll add more parameters, consider creating a HandlerConfig struct.
    pub fn new(substream_timeout: Duration) -> Self {
        Self {
            substream_timeout,
            request_to_responses_receiver: Default::default(),
            pending_events: Default::default(),
            ready_requests: Default::default(),
        }
    }

    fn convert_upgrade_error(
        &self,
        error: StreamUpgradeError<OutboundProtocolError>,
    ) -> RequestError {
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

    fn clear_pending_events_related_to_request(&mut self, request_id: RequestId) {
        self.pending_events.retain(|event| match event {
            ConnectionHandlerEvent::NotifyBehaviour(RequestProgressEvent::ReceivedResponse {
                request_id: other_request_id,
                ..
            }) => request_id != *other_request_id,
            _ => true,
        })
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = NewRequestEvent;
    type ToBehaviour = RequestProgressEvent;
    type Error = RemoteDoesntSupportProtocolError;
    type InboundProtocol = InboundProtocol;
    type OutboundProtocol = OutboundProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = RequestId;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        let (inbound_protocol, _) = InboundProtocol::new();
        SubstreamProtocol::new(inbound_protocol, ()).with_timeout(self.substream_timeout)
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
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }

        for (request_id, responses_receiver) in &mut self.request_to_responses_receiver {
            if let Poll::Ready(Some(response)) = responses_receiver.poll_next_unpin(cx) {
                // Collect all ready responses to avoid starvation of the request ids at the end.
                self.ready_requests.push_back((*request_id, response));
            }
        }
        if let Some((request_id, response)) = self.ready_requests.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                RequestProgressEvent::ReceivedResponse { request_id, response },
            ));
        }

        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        let NewRequestEvent { request, request_id } = event;
        let (outbound_protocol, responses_receiver) = OutboundProtocol::new(request);
        let insert_result =
            self.request_to_responses_receiver.insert(request_id, responses_receiver);
        if insert_result.is_some() {
            panic!("Multiple requests exist with the same ID {}", request_id);
        }
        self.pending_events.push_back(ConnectionHandlerEvent::OutboundSubstreamRequest {
            protocol: SubstreamProtocol::new(outbound_protocol, request_id)
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
                info: request_id,
            }) => {
                self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                    RequestProgressEvent::RequestFinished { request_id },
                ));
                self.request_to_responses_receiver.remove(&request_id);
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError { info: request_id, error }) => {
                let error = self.convert_upgrade_error(error);
                if matches!(error, RequestError::RemoteDoesntSupportProtocol) {
                    // This error will happen on all future connections to the peer, so we'll close
                    // the handle after reporting to the behaviour.
                    self.pending_events.clear();
                    self.pending_events.push_front(ConnectionHandlerEvent::NotifyBehaviour(
                        RequestProgressEvent::RequestFailed { request_id, error },
                    ));
                    self.pending_events
                        .push_back(ConnectionHandlerEvent::Close(RemoteDoesntSupportProtocolError));
                } else {
                    self.clear_pending_events_related_to_request(request_id);
                    self.pending_events.push_back(ConnectionHandlerEvent::NotifyBehaviour(
                        RequestProgressEvent::RequestFailed { request_id, error },
                    ));
                }
                self.request_to_responses_receiver.remove(&request_id);
            }
            ConnectionEvent::FullyNegotiatedInbound(_)
            | ConnectionEvent::ListenUpgradeError(_)
            | ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
        }
    }
}
