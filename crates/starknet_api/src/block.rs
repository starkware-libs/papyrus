use serde::{Deserialize, Serialize};

use super::serde_utils::{HexAsBytes, NonPrefixedHexAsBytes, PrefixedHexAsBytes};
use super::{ContractAddress, StarkHash, Transaction};

// TODO(spapini): Verify the invariant that it is in range.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHash(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "NonPrefixedHexAsBytes<32_usize>", into = "NonPrefixedHexAsBytes<32_usize>")]
pub struct GlobalRoot(pub StarkHash);
// We don't use the regular StarkHash deserialization since the Starknet sequencer returns the
// global root hash as a hex string without a "0x" prefix.
impl From<NonPrefixedHexAsBytes<32_usize>> for GlobalRoot {
    fn from(val: NonPrefixedHexAsBytes<32_usize>) -> Self {
        GlobalRoot(StarkHash(val.0))
    }
}
impl From<GlobalRoot> for NonPrefixedHexAsBytes<32_usize> {
    fn from(val: GlobalRoot) -> Self {
        HexAsBytes(val.0.0)
    }
}
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockNumber(pub u64);
impl BlockNumber {
    pub fn next(&self) -> BlockNumber {
        BlockNumber(self.0 + 1)
    }

    pub fn prev(&self) -> Option<BlockNumber> {
        match self.0 {
            0 => None,
            i => Some(BlockNumber(i - 1)),
        }
    }
}
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub u64);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedHexAsBytes<16_usize>", into = "PrefixedHexAsBytes<16_usize>")]
pub struct GasPrice(pub u128);
impl From<PrefixedHexAsBytes<16_usize>> for GasPrice {
    fn from(val: PrefixedHexAsBytes<16_usize>) -> Self {
        GasPrice(u128::from_be_bytes(val.0))
    }
}
impl From<GasPrice> for PrefixedHexAsBytes<16_usize> {
    fn from(val: GasPrice) -> Self {
        HexAsBytes(val.0.to_be_bytes())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ListCommitment {
    pub length: u64,
    pub commitment: StarkHash,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionsCommitment(pub ListCommitment);
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventsCommitment(pub ListCommitment);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl Default for BlockStatus {
    fn default() -> Self {
        BlockStatus::AcceptedOnL2
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub state_root: GlobalRoot,
    pub sequencer: ContractAddress,
    pub timestamp: BlockTimestamp,
    pub status: BlockStatus,
    // TODO(dan): add missing commitments.
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockBody {
    pub transactions: Vec<Transaction>,
}
