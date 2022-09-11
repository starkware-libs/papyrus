//! Representations of canonical [`StarkNet`] components.
//!
//! [`StarkNet`]: https://starknet.io/

mod block;
mod core;
mod hash;
pub mod serde_utils;
mod state;
mod transaction;

use serde_utils::DeserializationError;

pub use self::block::{
    Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockStatus, BlockTimestamp, GasPrice,
    GlobalRoot,
};
pub use self::core::{ClassHash, ContractAddress, Nonce};
pub use self::hash::{StarkFelt, StarkHash, GENESIS_HASH};
pub use self::state::{
    ContractNonce, DeclaredContract, DeployedContract, StateDiff, StateNumber, StorageDiff,
    StorageEntry, StorageKey,
};
pub use self::transaction::{
    CallData, ContractAddressSalt, ContractClass, DeclareTransaction, DeclareTransactionOutput,
    DeployTransaction, DeployTransactionOutput, EntryPoint, EntryPointOffset, EntryPointSelector,
    EntryPointType, EthAddress, Event, Fee, InvokeTransaction, InvokeTransactionOutput,
    L1HandlerTransaction, L1HandlerTransactionOutput, L1ToL2Payload, L2ToL1Payload, MessageToL1,
    MessageToL2, Program, Transaction, TransactionHash, TransactionOffsetInBlock,
    TransactionOutput, TransactionReceipt, TransactionSignature, TransactionVersion,
};

#[derive(thiserror::Error, Clone, Debug)]
pub enum StarknetApiError {
    #[error(transparent)]
    DeserializationError(#[from] DeserializationError),
    #[error("Out of range {string}.")]
    OutOfRange { string: String },
    #[error("Transactions and transaction outputs don't have the same length.")]
    TransationsLengthDontMatch,
}
