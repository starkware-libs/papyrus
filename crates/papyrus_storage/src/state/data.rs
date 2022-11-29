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
    deployed_contracts: Vec<(ContractAddress, ClassHash)>,
    storage_diffs: Vec<(ContractAddress, Vec<StorageEntry>)>,
    declared_contract_hashes: Vec<ClassHash>,
    nonces: Vec<(ContractAddress, Nonce)>,
}

impl ThinStateDiff {
    pub fn deployed_contracts(&self) -> &Vec<(ContractAddress, ClassHash)> {
        &self.deployed_contracts
    }
    pub fn storage_diffs(&self) -> &Vec<(ContractAddress, Vec<StorageEntry>)> {
        &self.storage_diffs
    }
    pub fn declared_contract_hashes(&self) -> &Vec<ClassHash> {
        &self.declared_contract_hashes
    }
    pub fn nonces(&self) -> &Vec<(ContractAddress, Nonce)> {
        &self.nonces
    }
}

impl From<StateDiff> for ThinStateDiff {
    fn from(diff: StateDiff) -> Self {
        let (deployed_contracts, storage_diffs, declared_classes, nonces) = diff.into();
        Self {
            deployed_contracts,
            storage_diffs,
            declared_contract_hashes: Vec::from_iter(
                declared_classes.into_iter().map(|(class_hash, _)| class_hash),
            ),
            nonces,
        }
    }
}
