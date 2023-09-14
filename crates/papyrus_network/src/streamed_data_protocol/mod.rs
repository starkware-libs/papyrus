pub mod behaviour;
pub mod handler;
pub mod protocol;

use derive_more::Display;
use prost::Message;

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct OutboundSessionId {
    pub value: usize,
}

#[derive(Clone, Copy, Debug, Default, Display, Eq, Hash, PartialEq)]
pub struct InboundSessionId {
    pub value: usize,
}

#[cfg_attr(test, derive(Debug, Clone, Eq, PartialEq, Copy))]
pub enum SessionId {
    Inbound(InboundSessionId),
    Outbound(OutboundSessionId),
}

impl Default for SessionId {
    fn default() -> Self {
        Self::Inbound(InboundSessionId::default())
    }
}

// This is a workaround for the unstable feature trait aliases
// https://doc.rust-lang.org/beta/unstable-book/language-features/trait-alias.html
pub(crate) trait QueryBound: Message + 'static + Default {}
impl<T> QueryBound for T where T: Message + 'static + Default {}

pub(crate) trait DataBound: Message + 'static + Unpin {}
impl<T> DataBound for T where T: Message + 'static + Unpin {}
