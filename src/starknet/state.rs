use serde::{Deserialize, Serialize};

use crate::starknet::hash::StarkFelt;
use crate::starknet::ContractAddress;

use super::StarkHash;

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(pub StarkHash);

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub addr: StarkFelt,
    pub value: StarkFelt,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    pub diff: Vec<StorageEntry>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffForward {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffBackward {
    pub deployed_contracts: Vec<ContractAddress>,
    pub storage_diffs: Vec<StorageDiff>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}
