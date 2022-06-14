use serde::{Deserialize, Serialize};

use super::hash::StarkHash;

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(pub StarkHash);
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHash(pub StarkHash);
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct GlobalRoot(pub StarkHash);
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
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct GasPrice(pub u128);
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ListCommitment {
    pub length: u64,
    pub commitment: StarkHash,
}
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionsCommitment(pub ListCommitment);
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EventsCommitment(pub ListCommitment);

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub number: BlockNumber,
    pub gas_price: GasPrice,
    pub state_root: GlobalRoot,
    pub sequencer: ContractAddress,
    pub timestamp: BlockTimestamp,
    // TODO(dan): add missing commitments.
}

pub struct BlockBody {}
