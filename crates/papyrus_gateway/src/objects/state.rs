use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, ClassHash, ContractAddress, DeployedContract, GlobalRoot, Nonce, StarkFelt,
    StorageDiff as StarknetStorageDiff, StorageKey,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    pub key: StorageKey,
    pub value: StarkFelt,
}

pub fn from_starknet_storage_diffs(storage_diffs: Vec<StarknetStorageDiff>) -> Vec<StorageDiff> {
    let mut res = vec![];
    for diff in storage_diffs {
        for storage_entry in diff.diff {
            res.push(StorageDiff {
                address: diff.address,
                key: storage_entry.key,
                value: storage_entry.value,
            });
        }
    }
    res
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractNonce {
    pub contract_address: ContractAddress,
    pub nonce: Nonce,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct GateWayStateDiff {
    pub storage_diffs: Vec<StorageDiff>,
    pub declared_classes: Vec<ClassHash>,
    pub deployed_contracts: Vec<DeployedContract>,
    pub nonces: Vec<ContractNonce>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: GateWayStateDiff,
}
