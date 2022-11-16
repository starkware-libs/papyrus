#[cfg(test)]
#[path = "core_test.rs"]
mod core_test;

use std::fmt;
use std::fmt::{Debug, Display};

use serde::{Deserialize, Serialize};

use super::{StarkFelt, StarkHash, StarknetApiError};

/// Starknet chain id.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ChainId(pub String);
impl Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl ChainId {
    pub fn as_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.0))
    }
}

/// 2**251
pub const PATRICIA_KEY_UPPER_BOUND: &str =
    "0x800000000000000000000000000000000000000000000000000000000000000";

// Invariant: key is in range
#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct PatriciaKey(StarkHash);
impl PatriciaKey {
    pub fn new(hash: StarkHash) -> Result<PatriciaKey, StarknetApiError> {
        if hash >= StarkHash::from_hex(PATRICIA_KEY_UPPER_BOUND)? {
            return Err(StarknetApiError::OutOfRange {
                string: format!("[0x0, {PATRICIA_KEY_UPPER_BOUND})"),
            });
        }
        Ok(PatriciaKey(hash))
    }
    pub fn key(&self) -> &StarkHash {
        &self.0
    }
}

impl Debug for PatriciaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PatriciaKey").field(&self.0).finish()
    }
}

/// The address of a StarkNet contract.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(PatriciaKey);

impl TryFrom<StarkHash> for ContractAddress {
    type Error = StarknetApiError;
    fn try_from(hash: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::new(hash)?))
    }
}

impl ContractAddress {
    pub fn contract_address(&self) -> &PatriciaKey {
        &self.0
    }
}

/// The hash of a StarkNet [ContractClass](`super::ContractClass`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(StarkHash);

impl ClassHash {
    pub fn new(hash: StarkHash) -> Self {
        Self(hash)
    }
    pub fn class_hash(&self) -> &StarkHash {
        &self.0
    }
}

/// The nonce of a StarkNet contract.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Nonce(StarkFelt);

impl Nonce {
    pub fn new(felt: StarkFelt) -> Self {
        Self(felt)
    }
    pub fn nonce(&self) -> &StarkFelt {
        &self.0
    }
}

impl Default for Nonce {
    fn default() -> Self {
        Nonce(StarkFelt::from_u64(0))
    }
}

/// The selector of an entry point in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointSelector(pub StarkHash);
