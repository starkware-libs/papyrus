#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::serde_utils::{
    bytes_from_hex_str, DeserializationError, HexAsBytes, PrefixedHexAsBytes,
};
#[derive(Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(
    from = "PrefixedHexAsBytes<32_usize>",
    into = "PrefixedHexAsBytes<32_usize>"
)]
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
    pub fn from_hex(hex_str: &str) -> Result<StarkHash, DeserializationError> {
        let bytes = bytes_from_hex_str::<32, true>(hex_str)?;
        Ok(StarkHash(bytes))
    }
    pub fn from_u8(val: u8) -> StarkHash {
        StarkHash([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, val,
        ])
    }
}

pub type StarkFelt = StarkHash;

#[allow(unused_macros)]
macro_rules! shash {
    ($s: expr) => {
        StarkHash::from_hex($s).unwrap()
    };
}
pub(crate) use shash;
