pub mod behaviour;
pub mod handler;
pub mod protocol;

#[cfg(test)]
mod flow_test;

use std::time::Duration;

use derive_more::Display;
use libp2p::swarm::StreamProtocol;
use libp2p::PeerId;
use prost::Message;

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct OutboundSessionId {
    pub value: usize,
}

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct InboundSessionId {
    pub value: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SessionId {
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

// This is a workaround for the unstable feature trait aliases
// https://doc.rust-lang.org/beta/unstable-book/language-features/trait-alias.html
pub(crate) trait QueryBound: Message + 'static + Default + Clone {}
impl<T> QueryBound for T where T: Message + 'static + Default + Clone {}

pub(crate) trait DataBound: Message + 'static + Unpin + Default {}
impl<T> DataBound for T where T: Message + 'static + Unpin + Default {}

#[derive(Debug)]
pub(crate) enum GenericEvent<Query: QueryBound, Data: DataBound, SessionError> {
    NewInboundSession { query: Query, inbound_session_id: InboundSessionId, peer_id: PeerId },
    ReceivedData { outbound_session_id: OutboundSessionId, data: Data },
    SessionFailed { session_id: SessionId, error: SessionError },
    SessionClosedByRequest { session_id: SessionId },
    SessionClosedByPeer { session_id: SessionId },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Config {
    pub substream_timeout: Duration,
    pub protocol_name: StreamProtocol,
}
