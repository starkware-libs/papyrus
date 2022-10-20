use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockNumber, ClassHash, ContractClass, ContractNonce, DeployedContract, StateDiff, StorageDiff,
};

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
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinStateDiff {
    deployed_contracts: Vec<DeployedContract>,
    storage_diffs: Vec<StorageDiff>,
    declared_contract_hashes: Vec<ClassHash>,
    nonces: Vec<ContractNonce>,
}
// move storage serde impl here.
impl ThinStateDiff {
    pub fn deployed_contracts(&self) -> &Vec<DeployedContract> {
        &self.deployed_contracts
    }
    pub fn storage_diffs(&self) -> &Vec<StorageDiff> {
        &self.storage_diffs
    }
    pub fn declared_contract_hashes(&self) -> &Vec<ClassHash> {
        &self.declared_contract_hashes
    }
    pub fn nonces(&self) -> &Vec<ContractNonce> {
        &self.nonces
    }
}

impl From<StateDiff> for ThinStateDiff {
    fn from(diff: StateDiff) -> Self {
        let (deployed_contracts, storage_diffs, declared_classes, nonces) = diff.destruct();
        Self {
            deployed_contracts,
            storage_diffs,
            declared_contract_hashes: Vec::from_iter(
                declared_classes.iter().map(|declared_contract| declared_contract.class_hash),
            ),
            nonces,
        }
    }
}
