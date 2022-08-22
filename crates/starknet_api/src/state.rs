use serde::{Deserialize, Serialize};

use super::{BlockNumber, ClassHash, ContractAddress, ContractClass, Nonce, StarkFelt};
use crate::StarknetApiError;

/// The sequential numbering of the states between blocks in StarkNet.
// Example:
// States: S0       S1       S2
// Blocks      B0->     B1->
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
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
/// The differences between two states in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateDiff {
    deployed_contracts: Vec<DeployedContract>,
    storage_diffs: Vec<StorageDiff>,
    declared_classes: Vec<(ClassHash, ContractClass)>,
    nonces: Vec<(ContractAddress, Nonce)>,
}

type StateDiffAsTuple = (
    Vec<DeployedContract>,
    Vec<StorageDiff>,
    Vec<(ClassHash, ContractClass)>,
    Vec<(ContractAddress, Nonce)>,
);

impl StateDiff {
    pub fn new(
        deployed_contracts: Vec<DeployedContract>,
        storage_diffs: Vec<StorageDiff>,
        declared_classes: Vec<(ClassHash, ContractClass)>,
        nonces: Vec<(ContractAddress, Nonce)>,
    ) -> Result<Self, StarknetApiError> {
        // TODO(yair): Use std::Vec::is_sorted_by_key once it becomes stable.
        let are_deployed_contrcats_sorted_by_address = std::iter::zip(
            deployed_contracts.iter().map(|i| i.address),
            deployed_contracts.iter().skip(1).map(|i| i.address),
        )
        .all(|addresses| addresses.0 < addresses.1);

        if !are_deployed_contrcats_sorted_by_address {
            return Err(StarknetApiError::DeployedContractsNotSorted);
        }

        let are_storage_diffs_sorted_by_address = std::iter::zip(
            storage_diffs.iter().map(|i| i.address),
            storage_diffs.iter().skip(1).map(|i| i.address),
        )
        .all(|addresses| addresses.0 < addresses.1);

        if !are_storage_diffs_sorted_by_address {
            return Err(StarknetApiError::StorageDiffsNotSorted);
        }

        let are_declared_classes_sorted_by_hash = std::iter::zip(
            declared_classes.iter().map(|i| i.0),
            declared_classes.iter().skip(1).map(|i| i.0),
        )
        .all(|hashes| hashes.0 < hashes.1);
        if !are_declared_classes_sorted_by_hash {
            return Err(StarknetApiError::DeclaredClassesNotSorted);
        }

        let are_nonces_sorted_by_address =
            std::iter::zip(nonces.iter().map(|i| i.0), nonces.iter().skip(1).map(|i| i.0))
                .all(|addresses| addresses.0 < addresses.1);
        if !are_nonces_sorted_by_address {
            return Err(StarknetApiError::NoncesNotSorted);
        }

        Ok(Self { deployed_contracts, storage_diffs, declared_classes, nonces })
    }

    pub fn destruct(self) -> StateDiffAsTuple {
        (self.deployed_contracts, self.storage_diffs, self.declared_classes, self.nonces)
    }
}

/// A deployed contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    address: ContractAddress,
    class_hash: ClassHash,
}

impl DeployedContract {
    pub fn new(address: ContractAddress, class_hash: ClassHash) -> Self {
        Self { address, class_hash }
    }
    pub fn address(&self) -> ContractAddress {
        self.address
    }

    pub fn class_hash(&self) -> ClassHash {
        self.class_hash
    }
}

/// A declared contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclaredContract {
    class_hash: ClassHash,
    contract_class: ContractClass,
}

impl DeclaredContract {
    pub fn new(class_hash: ClassHash, contract_class: ContractClass) -> Self {
        Self { class_hash, contract_class }
    }

    pub fn class_hash(&self) -> ClassHash {
        self.class_hash
    }

    pub fn contract_class(&self) -> &ContractClass {
        &self.contract_class
    }
}

/// Storage differences in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    address: ContractAddress,
    diff: Vec<StorageEntry>,
}

impl StorageDiff {
    pub fn new(address: ContractAddress, diff: Vec<StorageEntry>) -> Self {
        Self { address, diff }
    }

    pub fn address(&self) -> ContractAddress {
        self.address
    }

    pub fn diff(&self) -> &Vec<StorageEntry> {
        &self.diff
    }
}

// TODO: Invariant: this is in range.
// TODO(spapini): Enforce the invariant.
/// A storage key in a StarkNet contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageKey(pub StarkFelt);

/// A storage entry in a StarkNet contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    key: StorageKey,
    value: StarkFelt,
}

impl StorageEntry {
    pub fn new(key: StorageKey, value: StarkFelt) -> Self {
        Self { key, value }
    }
    pub fn key(&self) -> &StorageKey {
        &self.key
    }

    pub fn value(&self) -> &StarkFelt {
        &self.value
    }
}
