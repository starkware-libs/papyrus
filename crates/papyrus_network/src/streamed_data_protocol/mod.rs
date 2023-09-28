pub mod behaviour;
pub mod handler;
pub mod protocol;

use derive_more::Display;
use prost::Message;

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct OutboundSessionId {
    value: usize,
}

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct InboundSessionId {
    value: usize,
}

#[derive(Debug)]
// TODO(shahak) remove allow(dead_code).
#[allow(dead_code)]
pub(crate) enum SessionId {
    OutboundSessionId(OutboundSessionId),
    InboundSessionId(InboundSessionId),
}

// This is a workaround for the unstable feature trait aliases
// https://doc.rust-lang.org/beta/unstable-book/language-features/trait-alias.html
pub(crate) trait QueryBound: Message + 'static + Default {}
impl<T> QueryBound for T where T: Message + 'static + Default {}

pub(crate) trait DataBound: Message + 'static + Unpin + Default {}
impl<T> DataBound for T where T: Message + 'static + Unpin + Default {}

#[derive(Debug)]
// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
pub(crate) enum GenericEvent<Query: QueryBound, Data: DataBound, SessionError> {
    NewInboundSession { query: Query, inbound_session_id: InboundSessionId },
    ReceivedData { outbound_session_id: OutboundSessionId, data: Data },
    SessionFailed { session_id: SessionId, error: SessionError },
    OutboundSessionClosedByPeer { outbound_session_id: OutboundSessionId },
}
