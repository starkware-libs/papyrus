use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, BlockNumber, BlockTimestamp, ClassHash, ContractAddress, DeployedContract, GasPrice,
    GlobalRoot, NodeBlockStatus, StorageDiff, StorageEntry,
};

use super::transaction::{Transaction, TransactionReceipt};

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct Block {
    // TODO(dan): Currently should be Option<BlockHash> (due to pending blocks).
    // Figure out if we want this in the internal representation as well.
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub parent_block_hash: BlockHash,
    #[serde(default)]
    pub sequencer_address: ContractAddress,
    pub state_root: GlobalRoot,
    pub status: BlockStatus,
    #[serde(default)]
    pub timestamp: BlockTimestamp,
    pub transactions: Vec<Transaction>,
    pub transaction_receipts: Vec<TransactionReceipt>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct BlockStateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename(deserialize = "ABORTED", serialize = "ABORTED"))]
    Aborted,
    #[serde(rename(deserialize = "ACCEPTED_ON_L1", serialize = "ACCEPTED_ON_L1"))]
    AcceptedOnL1,
    #[serde(rename(deserialize = "ACCEPTED_ON_L2", serialize = "ACCEPTED_ON_L2"))]
    AcceptedOnL2,
    #[serde(rename(deserialize = "PENDING", serialize = "PENDING"))]
    Pending,
    #[serde(rename(deserialize = "REVERTED", serialize = "REVERTED"))]
    Reverted,
}
impl Default for BlockStatus {
    fn default() -> Self {
        BlockStatus::AcceptedOnL2
    }
}

impl From<BlockStatus> for NodeBlockStatus {
    fn from(status: BlockStatus) -> Self {
        match status {
            BlockStatus::Aborted => NodeBlockStatus::Rejected,
            BlockStatus::AcceptedOnL1 => NodeBlockStatus::AcceptedOnL1,
            BlockStatus::AcceptedOnL2 => NodeBlockStatus::AcceptedOnL2,
            BlockStatus::Pending => NodeBlockStatus::Pending,
            BlockStatus::Reverted => NodeBlockStatus::Rejected,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct StateDiff {
    pub storage_diffs: HashMap<ContractAddress, Vec<StorageEntry>>,
    pub deployed_contracts: Vec<DeployedContract>,
    #[serde(default)]
    pub declared_contracts: Vec<ClassHash>,
}

pub fn client_to_starknet_api_storage_diff(
    storage_diffs: HashMap<ContractAddress, Vec<StorageEntry>>,
) -> Vec<StorageDiff> {
    storage_diffs.into_iter().map(|(address, diff)| StorageDiff { address, diff }).collect()
}
