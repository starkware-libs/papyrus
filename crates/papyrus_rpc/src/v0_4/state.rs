use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::state::{
    EntryPoint,
    EntryPointType,
    StorageKey,
    ThinStateDiff as starknet_api_ThinStateDiff,
};
use starknet_client::reader::objects::state::{
    DeclaredClassHashEntry as ClientDeclaredClassHashEntry,
    DeployedContract as ClientDeployedContract,
    ReplacedClass as ClientReplacedClass,
    StateDiff as ClientStateDiff,
    StorageEntry as ClientStorageEntry,
};
use starknet_types_core::felt::Felt;

const CONTRACT_CLASS_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StateUpdate {
    AcceptedStateUpdate(AcceptedStateUpdate),
    PendingStateUpdate(PendingStateUpdate),
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct AcceptedStateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: ThinStateDiff,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingStateUpdate {
    pub old_root: GlobalRoot,
    pub state_diff: ThinStateDiff,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
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

impl From<ClientStateDiff> for ThinStateDiff {
    fn from(diff: ClientStateDiff) -> Self {
        Self {
            deployed_contracts: Vec::from_iter(diff.deployed_contracts.into_iter().map(
                |ClientDeployedContract { address, class_hash }| DeployedContract {
                    address,
                    class_hash,
                },
            )),
            storage_diffs: Vec::from_iter(diff.storage_diffs.into_iter().map(
                |(address, entries)| {
                    let storage_entries = Vec::from_iter(
                        entries
                            .into_iter()
                            .map(|ClientStorageEntry { key, value }| StorageEntry { key, value }),
                    );
                    StorageDiff { address, storage_entries }
                },
            )),
            declared_classes: diff
                .declared_classes
                .into_iter()
                .map(|ClientDeclaredClassHashEntry { class_hash, compiled_class_hash }| {
                    ClassHashes { class_hash, compiled_class_hash }
                })
                .collect(),
            deprecated_declared_classes: diff.old_declared_contracts,
            nonces: Vec::from_iter(
                diff.nonces
                    .into_iter()
                    .map(|(contract_address, nonce)| ContractNonce { contract_address, nonce }),
            ),
            replaced_classes: Vec::from_iter(diff.replaced_classes.into_iter().map(
                |ClientReplacedClass { address: contract_address, class_hash }| ReplacedClasses {
                    contract_address,
                    class_hash,
                },
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
    pub(super) storage_entries: Vec<StorageEntry>,
}

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: Felt,
}
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EntryPointByType {
    #[serde(rename = "CONSTRUCTOR")]
    pub constructor: Vec<EntryPoint>,
    #[serde(rename = "EXTERNAL")]
    pub external: Vec<EntryPoint>,
    #[serde(rename = "L1_HANDLER")]
    pub l1handler: Vec<EntryPoint>,
}

impl EntryPointByType {
    pub fn from_hash_map(entry_point_by_type: HashMap<EntryPointType, Vec<EntryPoint>>) -> Self {
        macro_rules! get_entrypoint_by_type {
            ($variant:ident) => {
                (*(entry_point_by_type.get(&EntryPointType::$variant).unwrap_or(&vec![]))).to_vec()
            };
        }

        Self {
            constructor: get_entrypoint_by_type!(Constructor),
            external: get_entrypoint_by_type!(External),
            l1handler: get_entrypoint_by_type!(L1Handler),
        }
    }
    pub fn to_hash_map(&self) -> HashMap<EntryPointType, Vec<EntryPoint>> {
        HashMap::from_iter([
            (EntryPointType::Constructor, self.constructor.clone()),
            (EntryPointType::External, self.external.clone()),
            (EntryPointType::L1Handler, self.l1handler.clone()),
        ])
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub sierra_program: Vec<Felt>,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

impl From<starknet_api::state::ContractClass> for ContractClass {
    fn from(class: starknet_api::state::ContractClass) -> Self {
        Self {
            sierra_program: class.sierra_program,
            contract_class_version: CONTRACT_CLASS_VERSION.to_owned(),
            entry_points_by_type: EntryPointByType::from_hash_map(class.entry_points_by_type),
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
