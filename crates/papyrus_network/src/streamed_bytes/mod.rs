pub mod behaviour;
pub mod handler;
mod messages;
pub mod protocol;

#[cfg(test)]
mod flow_test;

use std::time::Duration;

use derive_more::Display;
use libp2p::swarm::StreamProtocol;
use libp2p::PeerId;

pub type Bytes = Vec<u8>;

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct OutboundSessionId {
    pub value: usize,
}

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct InboundSessionId {
    pub value: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionId {
    OutboundSessionId(OutboundSessionId),
    InboundSessionId(InboundSessionId),
}

impl From<OutboundSessionId> for SessionId {
    fn from(outbound_session_id: OutboundSessionId) -> Self {
        Self::OutboundSessionId(outbound_session_id)
    }
}

impl From<InboundSessionId> for SessionId {
    fn from(inbound_session_id: InboundSessionId) -> Self {
        Self::InboundSessionId(inbound_session_id)
    }
}

#[derive(Debug)]
pub enum GenericEvent<SessionError> {
    NewInboundSession {
        query: Bytes,
        inbound_session_id: InboundSessionId,
        peer_id: PeerId,
        protocol_name: StreamProtocol,
    },
    ReceivedData {
        outbound_session_id: OutboundSessionId,
        data: Bytes,
    },
    SessionFailed {
        session_id: SessionId,
        error: SessionError,
    },
    SessionFinishedSuccessfully {
        session_id: SessionId,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Config {
    pub session_timeout: Duration,
    // If we put multiple versions of the same protocol, they should be inserted sorted where the
    // latest is the first (They don't have to appear continuously among the other protocols).
    // TODO(shahak): Sort protocols upon construction by version
    pub supported_inbound_protocols: Vec<StreamProtocol>,
}
