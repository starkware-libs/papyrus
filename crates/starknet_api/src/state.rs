use serde::{Deserialize, Serialize};

use super::{BlockNumber, ClassHash, ContractAddress, StarkFelt};

// Rerpesents the sequential numbering of the states between blocks.
// Example:
// States: S0       S1       S2
// Blocks      B0->     B1->
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StateNumber(u64);
impl StateNumber {
    // The state at the beginning of the block.
    pub fn right_before_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number.0)
    }
    // The state at the end of the block.
    pub fn right_after_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number.next().0)
    }
    pub fn is_before(&self, block_number: BlockNumber) -> bool {
        self.0 <= block_number.0
    }
    pub fn is_after(&self, block_number: BlockNumber) -> bool {
        self.0 > block_number.0
    }
    pub fn block_after(&self) -> BlockNumber {
        BlockNumber(self.0)
    }
}

// Invariant: Addresses are strictly increasing.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateDiffForward {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct IndexedDeployedContract {
    pub block_number: BlockNumber,
    pub class_hash: ClassHash,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclaredContract {
    pub class_hash: ClassHash,
}

// Invariant: Addresses are strictly increasing. In particular, no address appears twice.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    pub diff: Vec<StorageEntry>,
}

// TODO: Invariant: this is in range.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageKey(pub StarkFelt);

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}
