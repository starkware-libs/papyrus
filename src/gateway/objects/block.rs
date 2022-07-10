use serde::{Deserialize, Serialize};

use crate::starknet::BlockHeader as StarknetBlockHeader;
use crate::starknet::{
    BlockHash, BlockNumber, BlockTimestamp, ContractAddress, GlobalRoot, NodeBlockStatus,
};

use super::transaction::Transactions;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "PROVEN")]
    Proven,
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    #[serde(rename = "REJECTED")]
    Rejected,
}

impl From<NodeBlockStatus> for BlockStatus {
    fn from(status: NodeBlockStatus) -> Self {
        match status {
            NodeBlockStatus::Pending => BlockStatus::Pending,
            NodeBlockStatus::AcceptedOnL2 => BlockStatus::AcceptedOnL2,
            NodeBlockStatus::AcceptedOnL1 => BlockStatus::AcceptedOnL1,
            NodeBlockStatus::Rejected => BlockStatus::Rejected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub status: BlockStatus,
    pub sequencer: ContractAddress,
    pub new_root: GlobalRoot,
    pub accepted_time: BlockTimestamp,
}

impl From<StarknetBlockHeader> for BlockHeader {
    fn from(header: StarknetBlockHeader) -> Self {
        BlockHeader {
            block_hash: header.block_hash,
            parent_hash: header.parent_hash,
            block_number: header.number,
            status: header.status.into(),
            sequencer: header.sequencer,
            new_root: header.state_root,
            accepted_time: header.timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Transactions,
}
