use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{
    EntryPoint,
    EntryPointType,
    StorageKey,
    ThinStateDiff as starknet_api_ThinStateDiff,
};

const CONTRACT_CLASS_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: ThinStateDiff,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinStateDiff {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
    pub declared_classes: Vec<ClassHashes>,
    pub deprecated_declared_classes: Vec<ClassHash>,
    pub nonces: Vec<ContractNonce>,
    pub replaced_classes: Vec<ReplacedClasses>,
}

impl From<starknet_api_ThinStateDiff> for ThinStateDiff {
    fn from(diff: starknet_api_ThinStateDiff) -> Self {
        Self {
            deployed_contracts: Vec::from_iter(
                diff.deployed_contracts
                    .into_iter()
                    .map(|(address, class_hash)| DeployedContract { address, class_hash }),
            ),
            storage_diffs: Vec::from_iter(diff.storage_diffs.into_iter().map(
                |(address, entries)| {
                    let storage_entries = Vec::from_iter(
                        entries.into_iter().map(|(key, value)| StorageEntry { key, value }),
                    );
                    StorageDiff { address, storage_entries }
                },
            )),
            declared_classes: diff
                .declared_classes
                .into_iter()
                .map(|(class_hash, compiled_class_hash)| ClassHashes {
                    class_hash,
                    compiled_class_hash,
                })
                .collect(),
            deprecated_declared_classes: diff.deprecated_declared_classes,
            nonces: Vec::from_iter(
                diff.nonces
                    .into_iter()
                    .map(|(contract_address, nonce)| ContractNonce { contract_address, nonce }),
            ),
            replaced_classes: Vec::from_iter(diff.replaced_classes.into_iter().map(
                |(contract_address, class_hash)| ReplacedClasses { contract_address, class_hash },
            )),
        }
    }
}

/// The nonce of a StarkNet contract.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractNonce {
    pub contract_address: ContractAddress,
    pub nonce: Nonce,
}

/// A deployed contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

/// Storage differences in StarkNet.
// Invariant: Storage keys are strictly increasing. In particular, no key appears twice.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    storage_entries: Vec<StorageEntry>,
}

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub sierra_program: Vec<StarkFelt>,
    pub contract_class_version: String,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub abi: String,
}

impl From<starknet_api::state::ContractClass> for ContractClass {
    fn from(class: starknet_api::state::ContractClass) -> Self {
        Self {
            sierra_program: class.sierra_program,
            contract_class_version: CONTRACT_CLASS_VERSION.to_owned(),
            entry_points_by_type: class.entry_point_by_type,
            abi: class.abi,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ClassHashes {
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ReplacedClasses {
    pub contract_address: ContractAddress,
    pub class_hash: ClassHash,
}
