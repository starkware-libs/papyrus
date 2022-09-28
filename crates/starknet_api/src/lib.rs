//! Representations of canonical [`StarkNet`] components.
//!
//! [`StarkNet`]: https://starknet.io/

mod block;
mod core;
mod hash;
pub mod serde_utils;
mod state;
mod transaction;

use serde_utils::Deserialization;

pub use self::block::{
    Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockStatus, BlockTimestamp, GasPrice,
    GlobalRoot,
};
pub use self::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
pub use self::hash::{StarkFelt, StarkHash, GENESIS_HASH};
pub use self::state::{
    ContractClass, ContractNonce, DeclaredContract, DeployedContract, EntryPoint, EntryPointOffset,
    EntryPointType, Program, StateDiff, StateNumber, StorageDiff, StorageEntry, StorageKey,
};
pub use self::transaction::{
    CallData, ContractAddressSalt, DeclareTransaction, DeclareTransactionOutput, DeployTransaction,
    DeployTransactionOutput, EthAddress, Event, Fee, InvokeTransaction, InvokeTransactionOutput,
    L1HandlerTransaction, L1HandlerTransactionOutput, L1ToL2Payload, L2ToL1Payload, MessageToL1,
    MessageToL2, Transaction, TransactionHash, TransactionOffsetInBlock, TransactionOutput,
    TransactionReceipt, TransactionSignature, TransactionVersion,
};

#[derive(thiserror::Error, Clone, Debug)]
pub enum StarknetApiError {
    #[error(transparent)]
    Deserialization(#[from] Deserialization),
    #[error("Out of range {string}.")]
    OutOfRange { string: String },
    #[error("Transactions and transaction outputs don't have the same length.")]
    TransationsLengthDontMatch,
    #[error("Duplicate key in StateDiff: {object}.")]
    DuplicateInStateDiff { object: String },
}
