// pub mod behaviour;
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

// This is a workaround for the unstable feature trait aliases
// https://doc.rust-lang.org/beta/unstable-book/language-features/trait-alias.html
pub(crate) trait QueryBound: Message + 'static + Default {}
impl<T> QueryBound for T where T: Message + 'static + Default {}

pub(crate) trait DataBound: Message + 'static + Unpin {}
impl<T> DataBound for T where T: Message + 'static + Unpin {}
