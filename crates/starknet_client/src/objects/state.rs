use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;

use crate::GlobalRoot;

/// A state update derived from a single block as returned by the starknet gateway.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}

// TODO(yair): add #[serde(deny_unknown_fields)] once 0.11 is fully supported.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StateDiff {
    // IndexMap is serialized as a mapping in json, keeps ordering and is efficiently iterable.
    pub storage_diffs: IndexMap<ContractAddress, Vec<StorageEntry>>,
    pub deployed_contracts: Vec<DeployedContract>,
    #[serde(default)]
    pub old_declared_contracts: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
}

impl StateDiff {
    // Returns the declared class hashes and after them the deployed class hashes that weren't in
    // the declared.
    pub fn class_hashes(&self) -> Vec<ClassHash> {
        let mut deployed_class_hashes = self
            .deployed_contracts
            .iter()
            .map(|contract| contract.class_hash)
            .filter(|hash| !self.old_declared_contracts.contains(hash))
            .collect();
        let mut declared_class_hashes = self.old_declared_contracts.clone();
        declared_class_hashes.append(&mut deployed_class_hashes);
        declared_class_hashes
    }
}

/// A deployed contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}
