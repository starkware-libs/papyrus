use serde::{Deserialize, Serialize};

use crate::starknet::{BlockHash, BlockNumber, BlockTimestamp, ContractAddress, GlobalRoot};

use super::transaction::Transactions;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "PROVEN")]
    Proven,
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    #[serde(rename = "REJECTED")]
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub status: BlockStatus,
    pub sequencer: ContractAddress,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub accepted_time: BlockTimestamp,
    pub transactions: Transactions,
}
