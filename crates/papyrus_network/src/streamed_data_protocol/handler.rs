// #[cfg(test)]
// #[path = "handler_test.rs"]
// mod handler_test;

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use libp2p::swarm::handler::{
    ConnectionEvent,
    DialUpgradeError,
    FullyNegotiatedInbound,
    FullyNegotiatedOutbound,
};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, KeepAlive, SubstreamProtocol};

use super::protocol::{InboundProtocol, OutboundProtocol, PROTOCOL_NAME};
use super::{DataBound, InboundSessionId, OutboundSessionId, QueryBound};

#[derive(Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum SessionId {
    OutboundSessionId(OutboundSessionId),
    InboundSessionId(InboundSessionId),
}

#[derive(Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum RequestFromBehaviourEvent<Query, Data> {
    CreateOutboundSession { query: Query, outbound_session_id: OutboundSessionId },
    SendData { data: Data, inbound_session_id: InboundSessionId },
    FinishSession { session_id: SessionId },
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

#[derive(thiserror::Error, Debug)]
#[error("Remote peer doesn't support the {PROTOCOL_NAME} protocol.")]
pub(crate) struct RemoteDoesntSupportProtocolError;

#[derive(Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum ToBehaviourEvent<Query, Data> {
    NewInboundSession { query: Query, inbound_session_id: InboundSessionId },
    ReceivedData { outbound_session_id: OutboundSessionId, data: Data },
    SessionFailed { session_id: SessionId, error: SessionError },
}

type HandlerEvent<H> = ConnectionHandlerEvent<
    <H as ConnectionHandler>::OutboundProtocol,
    <H as ConnectionHandler>::OutboundOpenInfo,
    <H as ConnectionHandler>::ToBehaviour,
    <H as ConnectionHandler>::Error,
>;

// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) struct Handler<Query: QueryBound, Data: DataBound> {
    substream_timeout: Duration,
    pending_events: VecDeque<HandlerEvent<Self>>,
}

impl<Query: QueryBound, Data: DataBound> Handler<Query, Data> {
    // TODO(shahak) If we'll add more parameters, consider creating a HandlerConfig struct.
    // TODO(shahak) remove allow(dead_code).
    #[allow(dead_code)]
    pub fn new(_substream_timeout: Duration, _next_inbound_session_id: Arc<AtomicUsize>) -> Self {
        unimplemented!();
    }

    // fn convert_upgrade_error(
    //     &self,
    //     error: StreamUpgradeError<OutboundProtocolError<Data>>,
    // ) -> RequestError<Data> {
    //     match error {
    //         StreamUpgradeError::Timeout => {
    //             RequestError::Timeout { substream_timeout: self.substream_timeout }
    //         }
    //         StreamUpgradeError::Apply(request_protocol_error) => match request_protocol_error {
    //             OutboundProtocolError::IOError(error) => RequestError::IOError(error),
    //             OutboundProtocolError::ResponseSendError(error) => {
    //                 RequestError::ResponseSendError(error)
    //             }
    //         },
    //         StreamUpgradeError::NegotiationFailed => RequestError::RemoteDoesntSupportProtocol,
    //         StreamUpgradeError::Io(error) => RequestError::IOError(error),
    //     }
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
        unimplemented!();
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        // TODO(shahak): Implement keep alive logic.
        KeepAlive::Yes
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::ToBehaviour,
            Self::Error,
        >,
    > {
        unimplemented!();
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            RequestFromBehaviourEvent::CreateOutboundSession {
                query: _query,
                outbound_session_id: _outbound_session_id,
            } => {
                unimplemented!();
            }
            RequestFromBehaviourEvent::SendData {
                data: _data,
                inbound_session_id: _inbound_session_id,
            } => {
                unimplemented!();
            }
            RequestFromBehaviourEvent::FinishSession {
                session_id: SessionId::InboundSessionId(_inbound_session_id),
            } => {
                unimplemented!();
            }
            RequestFromBehaviourEvent::FinishSession {
                session_id: SessionId::OutboundSessionId(_outbound_session_id),
            } => {
                unimplemented!();
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
                protocol: _stream,
                info: _outbound_session_id,
            }) => {
                unimplemented!();
            }
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: (_query, _stream),
                info: _inbound_session_id,
            }) => {
                unimplemented!();
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
