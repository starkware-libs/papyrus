use serde::{Deserialize, Serialize};

use crate::starknet;

use super::serde_utils::HexAsBytes;

type PrefixedHexAsBytes<const N: usize> = HexAsBytes<N, true>;
type NonPrefixedHexAsBytes<const N: usize> = HexAsBytes<N, false>;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedHexAsBytes<32_usize>")]
pub struct StarkHash(pub [u8; 32]);
impl From<PrefixedHexAsBytes<32_usize>> for StarkHash {
    fn from(v: PrefixedHexAsBytes<32_usize>) -> Self {
        StarkHash(v.0)
    }
}
impl From<StarkHash> for starknet::StarkHash {
    fn from(val: StarkHash) -> Self {
        starknet::StarkHash(val.0)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHash(pub StarkHash);
impl From<BlockHash> for starknet::BlockHash {
    fn from(val: BlockHash) -> Self {
        starknet::BlockHash(val.0.into())
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractAddress(pub StarkHash);
impl From<ContractAddress> for starknet::ContractAddress {
    fn from(val: ContractAddress) -> Self {
        starknet::ContractAddress(val.0.into())
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(from = "NonPrefixedHexAsBytes<32_usize>")]
pub struct GlobalRoot(pub StarkHash);
// We don't use the regular StarkHash deserialization since the Starknet sequencer returns the
// global root hash as a hex string without a "0x" prefix.
impl From<NonPrefixedHexAsBytes<32_usize>> for GlobalRoot {
    fn from(val: NonPrefixedHexAsBytes<32_usize>) -> Self {
        GlobalRoot(StarkHash(val.0))
    }
}
impl From<GlobalRoot> for starknet::GlobalRoot {
    fn from(val: GlobalRoot) -> Self {
        starknet::GlobalRoot(val.0.into())
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockNumber(pub u64);
impl From<BlockNumber> for starknet::BlockNumber {
    fn from(val: BlockNumber) -> Self {
        starknet::BlockNumber(val.0)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(from = "PrefixedHexAsBytes<16_usize>")]
pub struct GasPrice(pub u128);
impl From<PrefixedHexAsBytes<16_usize>> for GasPrice {
    fn from(v: PrefixedHexAsBytes<16_usize>) -> Self {
        GasPrice(u128::from_be_bytes(v.0))
    }
}
impl From<GasPrice> for starknet::GasPrice {
    fn from(val: GasPrice) -> Self {
        starknet::GasPrice(val.0)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockTimestamp(pub u64);
impl From<BlockTimestamp> for starknet::BlockTimestamp {
    fn from(val: BlockTimestamp) -> Self {
        starknet::BlockTimestamp(val.0)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Block {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub parent_block_hash: BlockHash,
    pub sequencer_address: ContractAddress,
    pub state_root: GlobalRoot,
    pub status: BlockStatus,
    pub timestamp: BlockTimestamp,
    // TODO(dan): define corresponding structs and handle properly.
    transaction_receipts: Vec<serde_json::Value>,
    transactions: Vec<serde_json::Value>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename(deserialize = "ABORTED"))]
    Aborted,
    #[serde(rename(deserialize = "ACCEPTED_ON_L1"))]
    AcceptedOnL1,
    #[serde(rename(deserialize = "ACCEPTED_ON_L2"))]
    AcceptedOnL2,
    #[serde(rename(deserialize = "PENDING"))]
    Pending,
    #[serde(rename(deserialize = "REVERTED"))]
    Reverted,
}
