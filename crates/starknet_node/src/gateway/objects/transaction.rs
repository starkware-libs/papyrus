use serde::{Deserialize, Serialize};
use starknet_api::{Transaction, TransactionHash, TransactionReceipt};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionStatus {
    #[serde(rename = "UNKNOWN")]
    Unknown,
    #[serde(rename = "RECEIVED")]
    Received,
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    #[serde(rename = "REJECTED")]
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<Transaction>),
    FullAndReceipts(Vec<(Transaction, TransactionReceipt)>),
}
