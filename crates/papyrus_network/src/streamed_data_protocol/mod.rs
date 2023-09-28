//! This module contains implementation to a streamed data protocol.
//!
//! The protocol can send generic queries to other peers and receive a stream of generic data from
//! them.
//! The protocol also sends a stream of data to peers that've sent us a query.
//!
//! Whenever we send a query to another peer, an outbound session is created with a unique ID and
//! each data we receive related to that query is associated with that session.
//! Whenever we receive a query from another peer, an inbound session is created with a unique ID
//! and each data we send related to that query is associated with that session.
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

#[derive(Debug, PartialEq)]
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
    /// Another peer sent a query to us, and thus a new inbound session was created.
    NewInboundSession { query: Query, inbound_session_id: InboundSessionId },
    /// We received data from another peer on the given outbound session.
    ReceivedData { outbound_session_id: OutboundSessionId, data: Data },
    /// The given session (inbound or outbound) failed.
    SessionFailed { session_id: SessionId, error: SessionError },
    /// After we requested to close a session (inbound or outbound), this event reports that the
    /// session was closed successfully.
    SessionClosedByRequest { session_id: SessionId },
    /// An outbound session was closed because the other peer sent all the data.
    OutboundSessionClosedByPeer { outbound_session_id: OutboundSessionId },
}
