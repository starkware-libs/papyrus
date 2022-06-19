use serde::{Deserialize, Serialize};

use crate::starknet::{
    CallData, ContractAddress, EntryPointSelector, EthAddress, Event, Fee, L1ToL2Payload,
    L2ToL1Payload, TransactionHash,
};

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
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL2 {
    pub from_address: EthAddress,
    pub payload: L1ToL2Payload,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

/// `contract_address` field is available for Deploy and Invoke transactions.
/// `entry_point_selector` and `calldata` fields are available for Invoke transactions.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Transaction {
    pub contract_address: Option<ContractAddress>,
    pub entry_point_selector: Option<EntryPointSelector>,
    pub calldata: Option<CallData>,
    pub txn_hash: TransactionHash,
    pub max_fee: Fee,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceipt {
    transaction_hash: TransactionHash,
    actual_fee: Fee,
    status: TransactionStatus,
    status_data: String,
    messages_sent: Vec<MessageToL1>,
    l1_origin_message: Option<MessageToL2>,
    events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<Transaction>),
    FullAndReceipts(Vec<(Transaction, TransactionReceipt)>),
}
