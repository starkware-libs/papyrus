#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use serde::{Deserialize, Serialize};

use super::serde_utils::{
    bytes_from_hex_str, DeserializationError, HexAsBytes, PrefixedHexAsBytes,
};
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
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

impl StarkHash {
    pub fn from_hex(hex_str: &str) -> Result<StarkHash, DeserializationError> {
        let bytes = bytes_from_hex_str::<32, true>(hex_str)?;
        Ok(StarkHash(bytes))
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
