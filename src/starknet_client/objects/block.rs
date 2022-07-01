use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::starknet::{
    BlockHash, BlockNumber, BlockTimestamp, ContractAddress, DeployedContract, GasPrice,
    GlobalRoot, NodeBlockStatus, StateDiffForward as NodeStateDiff, StorageDiff, StorageEntry,
};

use super::{
    transaction::{Transaction, TransactionReceipt},
    NonPrefixedClassHash,
};

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
    pub deployed_contracts: Vec<NonPrefixedDeployedContract>,
    // TODO(dan): define corresponding struct and handle properly.
    #[serde(default)]
    pub declared_contracts: Vec<serde_json::Value>,
}

// TODO(dan): Simplify once clash_hash is always prefixed.
impl From<StateDiff> for NodeStateDiff {
    fn from(state_diff: StateDiff) -> Self {
        let deployed_contracts = state_diff
            .deployed_contracts
            .iter()
            .map(|x| DeployedContract::from(*x))
            .collect();
        let storage_diffs = state_diff
            .storage_diffs
            .iter()
            .map(|(&address, diff)| {
                let diff = diff.clone();
                StorageDiff { address, diff }
            })
            .collect();
        NodeStateDiff {
            deployed_contracts,
            storage_diffs,
        }
    }
}

// TODO(dan): Once clash_hash is always prefixed, revert and use Core DeployedContract.
#[derive(
    Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord, Copy,
)]
pub struct NonPrefixedDeployedContract {
    pub address: ContractAddress,
    pub class_hash: NonPrefixedClassHash,
}
impl From<NonPrefixedDeployedContract> for DeployedContract {
    fn from(val: NonPrefixedDeployedContract) -> Self {
        DeployedContract {
            address: val.address,
            class_hash: val.class_hash.into(),
        }
    }
}
