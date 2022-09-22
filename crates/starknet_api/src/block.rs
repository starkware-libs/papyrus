#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::fmt;
use std::vec::IntoIter;

use serde::{Deserialize, Serialize};

use super::serde_utils::{HexAsBytes, PrefixedHexAsBytes};
use super::{ContractAddress, StarkHash, StarknetApiError, Transaction, TransactionOutput};

// TODO(spapini): Verify the invariant that it is in range.
/// The hash of a StarkNet block.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHash(StarkHash);

impl BlockHash {
    pub fn new(hash: StarkHash) -> Self {
        Self(hash)
    }
    pub fn block_hash(&self) -> &StarkHash {
        &self.0
    }
}

/// The root of the global state at a StarkNet block.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct GlobalRoot(StarkHash);

impl GlobalRoot {
    pub fn new(hash: StarkHash) -> Self {
        Self(hash)
    }
    pub fn root(&self) -> &StarkHash {
        &self.0
    }
}

/// The block number of a StarkNet block.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockNumber(u64);
impl BlockNumber {
    pub const fn new(block_number: u64) -> Self {
        Self(block_number)
    }

    pub fn next(&self) -> BlockNumber {
        BlockNumber(self.0 + 1)
    }

    pub fn prev(&self) -> Option<BlockNumber> {
        match self.0 {
            0 => None,
            i => Some(BlockNumber(i - 1)),
        }
    }

    pub fn iter_up_to(&self, up_to: Self) -> impl Iterator<Item = BlockNumber> {
        let range = self.0..up_to.0;
        range.map(Self)
    }
    pub fn number(&self) -> &u64 {
        &self.0
    }
}

impl fmt::Display for BlockNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The timestamp of a StarkNet block.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(u64);
impl BlockTimestamp {
    pub fn new(time_stamp: u64) -> Self {
        Self(time_stamp)
    }
    pub fn time_stamp(&self) -> &u64 {
        &self.0
    }
}

/// The gas price at a StarkNet block.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedHexAsBytes<16_usize>", into = "PrefixedHexAsBytes<16_usize>")]
pub struct GasPrice(u128);
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

impl GasPrice {
    pub fn new(price: u128) -> Self {
        Self(price)
    }
    pub fn price(&self) -> &u128 {
        &self.0
    }
}

/// The status a StarkNet block.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    /// A pending block; i.e., a block that is yet to be closed.
    #[serde(rename = "PENDING")]
    Pending,
    /// A block that was created on L2.
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    /// A block that was accepted on L1.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    /// A block rejected on L1.
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl Default for BlockStatus {
    fn default() -> Self {
        BlockStatus::AcceptedOnL2
    }
}

/// The header of a StarkNet block.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    // TODO(anatg): Consider removing the block hash from the header (note it can be computed from
    // the rest of the fields.
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub state_root: GlobalRoot,
    pub sequencer: ContractAddress,
    pub timestamp: BlockTimestamp,
    // TODO(dan): add missing commitments.
}

/// The transactions in a StarkNet block.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockBody {
    transactions: Vec<Transaction>,
    transaction_outputs: Vec<TransactionOutput>,
}
impl BlockBody {
    pub fn new(
        transactions: Vec<Transaction>,
        transaction_outputs: Vec<TransactionOutput>,
    ) -> Result<Self, StarknetApiError> {
        if transactions.len() == transaction_outputs.len() {
            Ok(BlockBody { transactions, transaction_outputs })
        } else {
            Err(StarknetApiError::TransationsLengthDontMatch)
        }
    }

    pub fn transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    pub fn transaction_outputs(&self) -> &Vec<TransactionOutput> {
        &self.transaction_outputs
    }

    pub fn transaction_outputs_into_iter(self) -> IntoIter<TransactionOutput> {
        self.transaction_outputs.into_iter()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
}
