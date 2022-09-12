#[cfg(test)]
#[path = "core_test.rs"]
mod core_test;

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::serde_utils::PrefixedHexAsBytes;
use super::{StarkFelt, StarkHash, StarknetApiError};

/// 2**251
pub const PATRICIA_KEY_UPPER_BOUND: &str =
    "0x800000000000000000000000000000000000000000000000000000000000000";

#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub(crate) struct PatriciaKey(StarkHash);
impl PatriciaKey {
    pub fn new(hash: StarkHash) -> Result<PatriciaKey, StarknetApiError> {
        if hash >= StarkHash::from_hex(PATRICIA_KEY_UPPER_BOUND)? {
            return Err(StarknetApiError::OutOfRange {
                string: format!("[0x0, {PATRICIA_KEY_UPPER_BOUND})"),
            });
        }
        Ok(PatriciaKey(hash))
    }
}
impl TryFrom<PrefixedHexAsBytes<32_usize>> for PatriciaKey {
    type Error = StarknetApiError;
    fn try_from(val: PrefixedHexAsBytes<32_usize>) -> Result<Self, Self::Error> {
        let hash = StarkHash::new(val.0)?;
        PatriciaKey::new(hash)
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

/// The hash of a StarkNet [ContractClass](`super::ContractClass`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(StarkHash);

impl ClassHash {
    pub fn new(hash: StarkHash) -> Self {
        Self(hash)
    }
}

/// The nonce of a StarkNet contract.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Nonce(StarkFelt);

impl Nonce {
    pub fn new(felt: StarkFelt) -> Self {
        Self(felt)
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
