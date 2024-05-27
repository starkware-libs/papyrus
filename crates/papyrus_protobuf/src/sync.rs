use std::fmt::Debug;

use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

/// This struct represents a query that can be sent to a peer.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Query {
    pub start_block: BlockHashOrNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum BlockHashOrNumber {
    Hash(BlockHash),
    Number(BlockNumber),
}

impl Default for BlockHashOrNumber {
    fn default() -> Self {
        Self::Number(BlockNumber::default())
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeaderQuery(pub Query);

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateDiffQuery(pub Query);

// We have this struct in order to implement on it traits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeaderResponse(pub Option<SignedBlockHeader>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBlockHeader {
    pub block_header: BlockHeader,
    pub signatures: Vec<BlockSignature>,
}
