pub mod block;
#[cfg(test)]
pub mod objects_test_utils;
pub mod transactions;

use serde::{Deserialize, Serialize};

use crate::starknet;

use super::serde_utils::{HexAsBytes, PrefixedHexAsBytes};

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);
impl From<ContractAddress> for starknet::ContractAddress {
    fn from(val: ContractAddress) -> Self {
        starknet::ContractAddress(val.0.into())
    }
}

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
impl From<StarkHash> for starknet::StarkHash {
    fn from(val: StarkHash) -> Self {
        starknet::StarkHash(val.0)
    }
}
