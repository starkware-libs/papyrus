use papyrus_storage::state::data::ThinStateDiff as papyrus_storage_ThinStateDiff;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;

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
    pub deprecated_declared_classes: Vec<ClassHash>,
    pub nonces: Vec<ContractNonce>,
}

impl From<papyrus_storage_ThinStateDiff> for ThinStateDiff {
    fn from(diff: papyrus_storage_ThinStateDiff) -> Self {
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
            deprecated_declared_classes: diff.deprecated_declared_classes,
            nonces: Vec::from_iter(
                diff.nonces
                    .into_iter()
                    .map(|(contract_address, nonce)| ContractNonce { contract_address, nonce }),
            ),
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
