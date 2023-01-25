use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{ContractClass, StateDiff, StorageKey};

/// Data structs that are serialized into the database.

// Invariant: Addresses are strictly increasing.
// The invariant is enforced as [`ThinStateDiff`] is created only from [`starknet_api`][`StateDiff`]
// where the addresses are strictly increasing.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinStateDiff {
    pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
    pub declared_contract_hashes: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
}

impl ThinStateDiff {
    // Returns also the declared classes without cloning them.
    pub(crate) fn from_state_diff(diff: StateDiff) -> (Self, IndexMap<ClassHash, ContractClass>) {
        (
            Self {
                deployed_contracts: diff.deployed_contracts,
                storage_diffs: diff.storage_diffs,
                declared_contract_hashes: diff.declared_classes.keys().copied().collect(),
                nonces: diff.nonces,
            },
            diff.declared_classes,
        )
    }
}

impl From<StateDiff> for ThinStateDiff {
    fn from(diff: StateDiff) -> Self {
        Self::from_state_diff(diff).0
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub(crate) struct IndexedDeployedContract {
    pub block_number: BlockNumber,
    pub class_hash: ClassHash,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct IndexedDeclaredContract {
    pub block_number: BlockNumber,
    pub contract_class: ContractClass,
}
