#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::serde_utils::{
    bytes_from_hex_str, DeserializationError, HexAsBytes, PrefixedHexAsBytes,
};

/// A String slice representation of the genesis state hash.
pub const GENESIS_HASH: &str = "0x0";

// TODO: Move to a different crate.
/// A hash in StarkNet.
#[derive(Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(from = "PrefixedHexAsBytes<32_usize>", into = "PrefixedHexAsBytes<32_usize>")]
pub struct StarkHash(pub [u8; 32]);
impl From<PrefixedHexAsBytes<32_usize>> for StarkHash {
    fn from(val: PrefixedHexAsBytes<32_usize>) -> Self {
        StarkHash(val.0)
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
    /// Returns a [`StarkHash`] corresponding to `hex_str`.
    pub fn from_hex(hex_str: &str) -> Result<StarkHash, DeserializationError> {
        let bytes = bytes_from_hex_str::<32, true>(hex_str)?;
        Ok(StarkHash(bytes))
    }
    /// Returns a [`StarkHash`] corresponding to `val`.
    pub fn from_u64(val: u64) -> StarkHash {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&val.to_be_bytes());
        StarkHash(bytes)
    }
}

/// An alias for [`StarkHash`].
///
/// The StarkNet elliptic curve field element.
pub type StarkFelt = StarkHash;

/// A utility macro to create a [`StarkHash`] from a hex string representation.
#[macro_export]
macro_rules! shash {
    ($s:expr) => {
        StarkHash::from_hex($s).unwrap()
    };
}
