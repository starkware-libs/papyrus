use std::collections::HashMap;

use papyrus_storage::compression_utils::{encode, CompressionError};
use papyrus_storage::ThinStateDiff;
use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, ClassHash, ContractNonce, DeployedContract, EntryPoint, EntryPointType, GlobalRoot,
    StorageDiff,
};

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: serde_json::Value,
    pub program: String,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl TryFrom<starknet_api::ContractClass> for ContractClass {
    type Error = CompressionError;
    fn try_from(class: starknet_api::ContractClass) -> Result<Self, Self::Error> {
        Ok(Self {
            abi: class.abi,
            program: encode(class.program)?,
            entry_points_by_type: class.entry_points_by_type,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclaredContract {
    pub class_hash: ClassHash,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiff {
    pub storage_diffs: Vec<StorageDiff>,
    pub declared_contracts: Vec<DeclaredContract>,
    pub deployed_contracts: Vec<DeployedContract>,
    pub nonces: Vec<ContractNonce>,
}

impl From<ThinStateDiff> for StateDiff {
    fn from(diff: ThinStateDiff) -> Self {
        Self {
            storage_diffs: diff.storage_diffs,
            declared_contracts: diff
                .declared_classes
                .into_iter()
                .map(|class_hash| DeclaredContract { class_hash })
                .collect(),
            deployed_contracts: diff.deployed_contracts,
            nonces: diff.nonces,
        }
    }
}

impl From<starknet_api::StateDiff> for StateDiff {
    fn from(diff: starknet_api::StateDiff) -> Self {
        let (deployed_contracts, storage_diffs, declared_classes, nonces) = diff.destruct();
        Self {
            storage_diffs,
            declared_contracts: declared_classes
                .into_iter()
                .map(|declared_contract| DeclaredContract {
                    class_hash: declared_contract.class_hash,
                })
                .collect(),
            deployed_contracts,
            nonces,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}
