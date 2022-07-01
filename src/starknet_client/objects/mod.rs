use serde::{Deserialize, Serialize};

use crate::starknet::{
    serde_utils::{HexAsBytes, NonPrefixedHexAsBytes},
    ClassHash, StarkHash,
};

pub mod block;
pub mod transaction;

// TODO(dan): Once clash_hash is always prefixed, revert and use Core ClassHash.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(
    from = "NonPrefixedHexAsBytes<32_usize>",
    into = "NonPrefixedHexAsBytes<32_usize>"
)]
pub struct NonPrefixedClassHash(pub StarkHash);
impl From<NonPrefixedHexAsBytes<32_usize>> for NonPrefixedClassHash {
    fn from(val: NonPrefixedHexAsBytes<32_usize>) -> Self {
        NonPrefixedClassHash(StarkHash(val.0))
    }
}
impl From<NonPrefixedClassHash> for NonPrefixedHexAsBytes<32_usize> {
    fn from(val: NonPrefixedClassHash) -> Self {
        HexAsBytes(val.0 .0)
    }
}
impl From<NonPrefixedClassHash> for ClassHash {
    fn from(val: NonPrefixedClassHash) -> Self {
        ClassHash(val.0)
    }
}
