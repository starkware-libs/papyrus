use serde::{Deserialize, Serialize};

use super::{ContractAddress, StarkFelt, StarkHash};

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(pub StarkHash);

// Invariant: Addresses are strictly increasing.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffForward {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
}

// Invariant: Addresses are strictly increasing. In particular, no address appears twice.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffBackward {
    pub deployed_contracts: Vec<ContractAddress>,
    pub storage_diffs: Vec<StorageDiff>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

// Invariant: Addresses are strictly increasing. In particular, no address appears twice.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    address: ContractAddress,
    diff: Vec<StorageEntry>,
}

// TODO: Invariant: this is in range.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageKey(pub StarkFelt);

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    #[serde(rename(deserialize = "key", serialize = "key"))]
    pub addr: StorageKey,
    pub value: StarkFelt,
}
