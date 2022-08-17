use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, BlockNumber, BlockStatus, BlockTimestamp, ContractAddress, GlobalRoot,
};

use super::transaction::Transactions;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub status: BlockStatus,
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
            status: header.status,
            sequencer_address: header.sequencer,
            new_root: header.state_root,
            timestamp: header.timestamp,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Transactions,
}
