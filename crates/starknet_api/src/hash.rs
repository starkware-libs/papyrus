#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::serde_utils::{
    bytes_from_hex_str, DeserializationError, HexAsBytes, PrefixedHexAsBytes,
};
use crate::serde_utils::hex_str_from_bytes;

/// Genesis state hash.
pub const GENESIS_HASH: &str = "0x0";
/// 2**251 - 256 - 1
pub const PATRICIA_KEY_UPPER_BOUND: &str =
    "0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeff";

// TODO: Move to a different crate.
/// A hash in StarkNet.
#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(try_from = "PrefixedHexAsBytes<32_usize>", into = "PrefixedHexAsBytes<32_usize>")]
pub struct StarkHash([u8; 32]);
impl StarkHash {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}
impl TryFrom<PrefixedHexAsBytes<32_usize>> for StarkHash {
    type Error = DeserializationError;
    fn try_from(val: PrefixedHexAsBytes<32_usize>) -> Result<Self, Self::Error> {
        StarkHash::new(val.0)
    }
}
impl From<StarkHash> for PrefixedHexAsBytes<32_usize> {
    fn from(val: StarkHash) -> Self {
        HexAsBytes(val.0)
    }
}

impl Debug for StarkHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("0x{}", hex::encode(&self.0));
        f.debug_tuple("StarkHash").field(&s).finish()
    }
}

impl StarkHash {
    /// Returns a new [`StarkHash`].
    pub fn new(bytes: [u8; 32]) -> Result<StarkHash, DeserializationError> {
        if bytes[0] >= 0x10 {
            return Err(DeserializationError::OutOfRange {
                string: hex_str_from_bytes::<32, true>(bytes),
            });
        }
        Ok(Self(bytes))
    }
    /// Returns a [`StarkHash`] corresponding to `hex_str`.
    pub fn from_hex(hex_str: &str) -> Result<StarkHash, DeserializationError> {
        let bytes = bytes_from_hex_str::<32, true>(hex_str)?;
        Self::new(bytes)
    }
    /// Returns a [`StarkHash`] corresponding to `val`.
    pub fn from_u64(val: u64) -> StarkHash {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&val.to_be_bytes());
        StarkHash(bytes)
    }
}

/// The StarkNet elliptic curve field element.
pub type StarkFelt = StarkHash;

/// A utility macro to create a [`StarkHash`] from a hex string representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! shash {
    ($s:expr) => {
        StarkHash::from_hex($s).unwrap()
    };
}

#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(try_from = "PrefixedHexAsBytes<32_usize>", into = "PrefixedHexAsBytes<32_usize>")]
pub struct PatriciaKey(StarkHash);
impl PatriciaKey {
    pub fn new(hash: StarkHash) -> Result<PatriciaKey, DeserializationError> {
        if hash > shash!(PATRICIA_KEY_UPPER_BOUND) {
            return Err(DeserializationError::OutOfRange {
                string: format!("[0x0, {PATRICIA_KEY_UPPER_BOUND})"),
            });
        }
        Ok(PatriciaKey(hash))
    }
    pub fn hash(&self) -> &StarkHash {
        &self.0
    }
    pub fn into_hash(self) -> StarkHash {
        self.0
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
        HexAsBytes(val.hash().into_bytes())
    }
}

impl Debug for PatriciaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PatriciaKey").field(&self.hash()).finish()
    }
}
