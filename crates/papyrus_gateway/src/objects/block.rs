use std::convert::TryFrom;

use serde::{Deserialize, Serialize};
use starknet_api::serde_utils::{HexAsBytes, PrefixedHexAsBytes};
use starknet_api::{
    BlockHash, BlockNumber, BlockStatus, BlockTimestamp, ContractAddress, StarkHash,
    StarknetApiError,
};

use super::transaction::Transactions;

#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(try_from = "PrefixedHexAsBytes<32_usize>", into = "PrefixedHexAsBytes<32_usize>")]
pub struct GlobalRoot(pub StarkHash);
impl TryFrom<PrefixedHexAsBytes<32_usize>> for GlobalRoot {
    type Error = StarknetApiError;
    fn try_from(val: PrefixedHexAsBytes<32_usize>) -> Result<Self, Self::Error> {
        Ok(GlobalRoot(StarkHash::try_from(val)?))
    }
}
impl From<GlobalRoot> for PrefixedHexAsBytes<32_usize> {
    fn from(val: GlobalRoot) -> Self {
        HexAsBytes(val.0.into_bytes())
    }
}
impl From<starknet_api::GlobalRoot> for GlobalRoot {
    fn from(val: starknet_api::GlobalRoot) -> Self {
        // Should not fail.
        Self::try_from(PrefixedHexAsBytes::from(val)).unwrap()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub sequencer_address: ContractAddress,
    pub new_root: GlobalRoot,
    pub timestamp: BlockTimestamp,
}

impl From<starknet_api::BlockHeader> for BlockHeader {
    fn from(header: starknet_api::BlockHeader) -> Self {
        BlockHeader {
            block_hash: header.block_hash,
            parent_hash: header.parent_hash,
            block_number: header.block_number,
            sequencer_address: header.sequencer,
            new_root: header.state_root.into(),
            timestamp: header.timestamp,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    pub status: BlockStatus,
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Transactions,
}
