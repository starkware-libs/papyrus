use std::collections::HashMap;

use indexmap::IndexMap;
pub use papyrus_common::state::{
    DeclaredClassHashEntry,
    DeployedContract,
    ReplacedClass,
    StorageEntry,
};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::state::{EntryPoint, EntryPointType};
use starknet_types_core::felt::Felt;

/// A state update derived from a single block as returned by the starknet gateway.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StateDiff {
    // IndexMap is serialized as a mapping in json, keeps ordering and is efficiently iterable.
    pub storage_diffs: IndexMap<ContractAddress, Vec<StorageEntry>>,
    pub deployed_contracts: Vec<DeployedContract>,
    pub declared_classes: Vec<DeclaredClassHashEntry>,
    pub old_declared_contracts: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
    pub replaced_classes: Vec<ReplacedClass>,
}

impl StateDiff {
    // Returns the declared class hashes in the following order:
    // [declared classes, deprecated declared class, class hashes of deployed contracts].
    pub fn class_hashes(&self) -> Vec<ClassHash> {
        let mut declared_class_hashes: Vec<ClassHash> = self
            .declared_classes
            .iter()
            .map(|DeclaredClassHashEntry { class_hash, compiled_class_hash: _ }| *class_hash)
            .collect();
        declared_class_hashes.append(&mut self.old_declared_contracts.clone());
        let mut deployed_class_hashes = self
            .deployed_contracts
            .iter()
            .map(|contract| contract.class_hash)
            .filter(|hash| !declared_class_hashes.contains(hash))
            .collect();
        declared_class_hashes.append(&mut deployed_class_hashes);
        declared_class_hashes
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub sierra_program: Vec<Felt>,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub contract_class_version: String,
    pub abi: String,
}

impl From<ContractClass> for starknet_api::state::ContractClass {
    fn from(class: ContractClass) -> Self {
        Self {
            sierra_program: class.sierra_program,
            entry_points_by_type: class.entry_points_by_type,
            abi: class.abi,
        }
    }
}
