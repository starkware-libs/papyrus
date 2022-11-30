//! Representations of canonical [`StarkNet`] components.
//!
//! [`StarkNet`]: https://starknet.io/

pub mod block;
pub mod core;
pub mod hash;
pub mod serde_utils;
pub mod state;
pub mod transaction;

use serde_utils::InnerDeserializationError;

/// The error type returned by StarknetApi.
#[derive(thiserror::Error, Clone, Debug)]
pub enum StarknetApiError {
    /// Error in the inner deserialization of the node.
    #[error(transparent)]
    InnerDeserialization(#[from] InnerDeserializationError),
    #[error("Out of range {string}.")]
    /// An error for when a value is out of range.
    OutOfRange { string: String },
    /// An error for when the [transactions](`crate::transaction::Transaction`) and the
    /// [transaction outputs](`crate::transaction::TransactionOutput`) in a StarkNet
    /// [block](`crate::block::Block`) don't match in size.
    #[error("Transactions and transaction outputs don't have the same length.")]
    TransactionsLengthDontMatch,
}
