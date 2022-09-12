use papyrus_storage::ThinStateDiff;
use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, ClassHash, ContractNonce, DeployedContract, GlobalRoot, StorageDiff,
};

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
