use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockNumber, ClassHash, ContractNonce, DeclaredContract, DeployedContract, StateDiff,
    StorageDiff,
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
    pub contract_class: Vec<u8>,
}

// Invariant: Addresses are strictly increasing.
// The invariant is enforced as [`ThinStateDiff`] is created only by the [`split_diff_for_storage`]
// function from a [`starknet_api`][`StateDiff`] where the addresses are strictly increasing.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinStateDiff {
    deployed_contracts: Vec<DeployedContract>,
    storage_diffs: Vec<StorageDiff>,
    declared_contract_hashes: Vec<ClassHash>,
    nonces: Vec<ContractNonce>,
}

pub(crate) fn split_diff_for_storage(
    state_diff: StateDiff,
    deployed_contract_class_definitions: Vec<DeclaredContract>,
) -> (ThinStateDiff, Vec<DeclaredContract>) {
    let (deployed_contracts, storage_diffs, mut declared_classes, nonces) = state_diff.destruct();
    let thin_state_diff = ThinStateDiff {
        deployed_contracts,
        storage_diffs,
        declared_contract_hashes: Vec::from_iter(
            declared_classes.iter().map(|declared_contract| declared_contract.class_hash),
        ),
        nonces,
    };
    declared_classes.extend(deployed_contract_class_definitions.into_iter());
    (thin_state_diff, declared_classes)
}

type ThinStateDiffAsTuple =
    (Vec<DeployedContract>, Vec<StorageDiff>, Vec<ClassHash>, Vec<ContractNonce>);

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
    pub fn destruct(self) -> ThinStateDiffAsTuple {
        (self.deployed_contracts, self.storage_diffs, self.declared_contract_hashes, self.nonces)
    }
}

#[cfg(any(feature = "testing", test))]
impl From<starknet_api::StateDiff> for ThinStateDiff {
    fn from(diff: starknet_api::StateDiff) -> Self {
        let (deployed_contracts, storage_diffs, declared_classes, nonces) = diff.destruct();
        Self {
            storage_diffs,
            declared_contract_hashes: declared_classes
                .into_iter()
                .map(|declared_contract| declared_contract.class_hash)
                .collect(),
            deployed_contracts,
            nonces,
        }
    }
}
