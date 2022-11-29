use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::{ContractClass, StateDiff, StorageEntry};

// Data structs that are serialized into the database.

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct IndexedDeployedContract {
    pub block_number: BlockNumber,
    pub class_hash: ClassHash,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct IndexedDeclaredContract {
    pub block_number: BlockNumber,
    pub contract_class: ContractClass,
}

// Invariant: Addresses are strictly increasing.
// The invariant is enforced as [`ThinStateDiff`] is created only from [`starknet_api`][`StateDiff`]
// where the addresses are strictly increasing.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinStateDiff {
    pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    pub storage_diffs: IndexMap<ContractAddress, Vec<StorageEntry>>,
    pub declared_contract_hashes: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
}

impl From<StateDiff> for ThinStateDiff {
    fn from(diff: StateDiff) -> Self {
        Self {
            deployed_contracts: diff.deployed_contracts,
            storage_diffs: diff.storage_diffs,
            declared_contract_hashes: diff.declared_classes.into_keys().collect(),
            nonces: diff.nonces,
        }
    }
}
