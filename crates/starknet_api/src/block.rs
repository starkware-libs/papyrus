use serde::{Deserialize, Serialize};

use super::serde_utils::{HexAsBytes, NonPrefixedHexAsBytes, PrefixedHexAsBytes};
use super::{ContractAddress, StarkHash, Transaction};
use crate::TransactionOutput;

// TODO(spapini): Verify the invariant that it is in range.
/// The hash of a StarkNet block.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHash(pub StarkHash);

/// The root of the global state at a StarkNet block.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
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

/// The block number of a StarkNet block.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
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

/// The timestamp of a StarkNet block.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub u64);

/// The gas price at a StarkNet block.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
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
    block_hash: BlockHash,
    parent_hash: BlockHash,
    block_number: BlockNumber,
    gas_price: GasPrice,
    state_root: GlobalRoot,
    sequencer: ContractAddress,
    timestamp: BlockTimestamp,
    status: BlockStatus,
    // TODO(dan): add missing commitments.
}

impl BlockHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        block_hash: BlockHash,
        parent_hash: BlockHash,
        block_number: BlockNumber,
        gas_price: GasPrice,
        state_root: GlobalRoot,
        sequencer: ContractAddress,
        timestamp: BlockTimestamp,
        status: BlockStatus,
    ) -> Self {
        BlockHeader {
            block_hash,
            parent_hash,
            block_number,
            gas_price,
            state_root,
            sequencer,
            timestamp,
            status,
        }
    }
    pub fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    pub fn parent_hash(&self) -> BlockHash {
        self.parent_hash
    }

    pub fn state_root(&self) -> GlobalRoot {
        self.state_root
    }

    #[cfg(any(feature = "testing", test))]
    pub fn block_number(&self) -> BlockNumber {
        self.block_number
    }

    pub fn destruct(
        self,
    ) -> (
        BlockHash,
        BlockHash,
        BlockNumber,
        GasPrice,
        GlobalRoot,
        ContractAddress,
        BlockTimestamp,
        BlockStatus,
    ) {
        (
            self.block_hash,
            self.parent_hash,
            self.block_number,
            self.gas_price,
            self.state_root,
            self.sequencer,
            self.timestamp,
            self.status,
        )
    }
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
    ) -> Self {
        Self { transactions, transaction_outputs }
    }

    pub fn transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    pub fn transaction_outputs(&self) -> &Vec<TransactionOutput> {
        &self.transaction_outputs
    }
}
