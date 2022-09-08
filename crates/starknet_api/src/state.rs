#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::serde_utils::{DeserializationError, HexAsBytes, PrefixedHexAsBytes};
use super::{BlockNumber, ClassHash, ContractAddress, ContractClass, Nonce, StarkFelt, StarkHash};

/// 2**251
pub const PATRICIA_KEY_UPPER_BOUND: &str =
    "0x800000000000000000000000000000000000000000000000000000000000000";

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

#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(try_from = "PrefixedHexAsBytes<32_usize>", into = "PrefixedHexAsBytes<32_usize>")]
pub struct PatriciaKey(StarkHash);
impl PatriciaKey {
    pub fn new(hash: StarkHash) -> Result<PatriciaKey, DeserializationError> {
        if hash >= StarkHash::from_hex(PATRICIA_KEY_UPPER_BOUND)? {
            return Err(DeserializationError::OutOfRange {
                string: format!("[0x0, {PATRICIA_KEY_UPPER_BOUND})"),
            });
        }
        Ok(PatriciaKey(hash))
    }
}
impl TryFrom<PrefixedHexAsBytes<32_usize>> for PatriciaKey {
    type Error = DeserializationError;
    fn try_from(val: PrefixedHexAsBytes<32_usize>) -> Result<Self, Self::Error> {
        let hash = StarkHash::new(val.0)?;
        PatriciaKey::new(hash)
    }
}
impl From<PatriciaKey> for PrefixedHexAsBytes<32_usize> {
    fn from(val: PatriciaKey) -> Self {
        HexAsBytes(val.0.into_bytes())
    }
}

impl Debug for PatriciaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PatriciaKey").field(&self.0).finish()
    }
}

// TODO(anatg): Consider replacing this with ThinStateDiff (that is, remove ContractClass)
// and append contract classes to the storage separately.
// Invariant: Addresses are strictly increasing.
/// The differences between two states in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateDiff {
    deployed_contracts: Vec<DeployedContract>,
    storage_diffs: Vec<StorageDiff>,
    declared_classes: Vec<DeclaredContract>,
    nonces: Vec<ContractNonce>,
}

type StateDiffAsTuple =
    (Vec<DeployedContract>, Vec<StorageDiff>, Vec<DeclaredContract>, Vec<ContractNonce>);

impl StateDiff {
    pub fn new(
        mut deployed_contracts: Vec<DeployedContract>,
        mut storage_diffs: Vec<StorageDiff>,
        mut declared_contracts: Vec<DeclaredContract>,
        mut nonces: Vec<ContractNonce>,
    ) -> Self {
        deployed_contracts.sort_by_key(|dc| dc.address);
        storage_diffs.sort_by_key(|sd| sd.address);
        declared_contracts.sort_by_key(|dc| dc.class_hash);
        nonces.sort_by_key(|n| n.contract_address);
        Self { deployed_contracts, storage_diffs, declared_classes: declared_contracts, nonces }
    }

    pub fn destruct(self) -> StateDiffAsTuple {
        (self.deployed_contracts, self.storage_diffs, self.declared_classes, self.nonces)
    }
}

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

/// A declared contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclaredContract {
    pub class_hash: ClassHash,
    pub contract_class: ContractClass,
}

// Invariant: Addresses are strictly increasing. In particular, no address appears twice.
// TODO(spapini): Enforce the invariant.
/// Storage differences in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    pub storage_entries: Vec<StorageEntry>,
}

// TODO: Invariant: this is in range.
// TODO(spapini): Enforce the invariant.
/// A storage key in a StarkNet contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageKey(pub PatriciaKey);

/// A storage entry in a StarkNet contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}
