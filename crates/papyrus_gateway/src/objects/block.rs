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
        let (
            block_hash,
            parent_hash,
            block_number,
            _gas_price,
            state_root,
            sequencer,
            timestamp,
            status,
        ) = header.destruct();
        BlockHeader {
            block_hash,
            parent_hash,
            block_number,
            status,
            sequencer_address: sequencer,
            new_root: state_root,
            timestamp,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Transactions,
}
