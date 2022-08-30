use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, BlockNumber, BlockStatus, Transaction, TransactionHash, TransactionReceipt,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TransactionWithType>),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "INVOKE", serialize = "INVOKE"))]
    Invoke,
}
impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::Invoke
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionWithType {
    pub r#type: TransactionType,
    #[serde(flatten)]
    pub transaction: Transaction,
}

impl From<Transaction> for TransactionWithType {
    fn from(transaction: Transaction) -> Self {
        match transaction {
            Transaction::Declare(_) => {
                TransactionWithType { r#type: TransactionType::Declare, transaction }
            }
            Transaction::Deploy(_) => {
                TransactionWithType { r#type: TransactionType::Deploy, transaction }
            }
            Transaction::Invoke(_) => {
                TransactionWithType { r#type: TransactionType::Invoke, transaction }
            }
        }
    }
}

/// A transaction status in StarkNet.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionStatus {
    /// The transaction passed the validation and entered the pending block.
    #[serde(rename = "PENDING")]
    Pending,
    /// The transaction passed the validation and entered an actual created block.
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    /// The transaction was accepted on-chain.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    /// The transaction failed validation.
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::AcceptedOnL2
    }
}

impl From<BlockStatus> for TransactionStatus {
    fn from(status: BlockStatus) -> Self {
        match status {
            BlockStatus::AcceptedOnL1 => TransactionStatus::AcceptedOnL1,
            BlockStatus::AcceptedOnL2 => TransactionStatus::AcceptedOnL2,
            BlockStatus::Pending => TransactionStatus::Pending,
            BlockStatus::Rejected => TransactionStatus::Rejected,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceiptWithStatus {
    pub status: TransactionStatus,
    #[serde(flatten)]
    pub receipt: TransactionReceipt,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub event: starknet_api::Event,
}
