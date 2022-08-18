use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, BlockNumber, BlockStatus, BlockTimestamp, ContractAddress, GlobalRoot,
};

use super::transaction::Transactions;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    block_hash: BlockHash,
    parent_hash: BlockHash,
    block_number: BlockNumber,
    status: BlockStatus,
    sequencer_address: ContractAddress,
    new_root: GlobalRoot,
    timestamp: BlockTimestamp,
}

impl BlockHeader {
    pub fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    #[cfg(test)]
    pub fn block_number(&self) -> BlockNumber {
        self.block_number
    }
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
    header: BlockHeader,
    transactions: Transactions,
}

impl Block {
    pub fn new(header: BlockHeader, transactions: Transactions) -> Self {
        Block { header, transactions }
    }

    #[cfg(test)]
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }
}
