use serde::{Deserialize, Serialize};

use crate::starknet::{
    serde_utils::{HexAsBytes, NonPrefixedHexAsBytes},
    ClassHash as OtherClassHash, StarkHash,
};

pub mod block;
pub mod transaction;

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(
    from = "NonPrefixedHexAsBytes<32_usize>",
    into = "NonPrefixedHexAsBytes<32_usize>"
)]
pub struct ClassHash(pub StarkHash);
impl From<NonPrefixedHexAsBytes<32_usize>> for ClassHash {
    fn from(val: NonPrefixedHexAsBytes<32_usize>) -> Self {
        ClassHash(StarkHash(val.0))
    }
}
impl From<ClassHash> for NonPrefixedHexAsBytes<32_usize> {
    fn from(val: ClassHash) -> Self {
        HexAsBytes(val.0 .0)
    }
}
impl From<ClassHash> for OtherClassHash {
    fn from(val: ClassHash) -> Self {
        OtherClassHash(val.0)
    }
}
