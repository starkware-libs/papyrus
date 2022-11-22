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

#[derive(thiserror::Error, Clone, Debug)]
pub enum StarknetApiError {
    #[error(transparent)]
    InnerDeserialization(#[from] InnerDeserializationError),
    #[error("Out of range {string}.")]
    OutOfRange { string: String },
    #[error("Transactions and transaction outputs don't have the same length.")]
    TransactionsLengthDontMatch,
    #[error("Duplicate key in StateDiff: {object}.")]
    DuplicateInStateDiff { object: String },
    #[error("Duplicate key in StorageEntry.")]
    DuplicateStorageEntry,
}
