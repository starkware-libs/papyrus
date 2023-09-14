mod executor;
/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod messages;
pub mod streamed_data_protocol;

use starknet_api::block::{Block, BlockHash, BlockNumber};
use streamed_data_protocol::InboundSessionId;

#[derive(Default)]
#[cfg_attr(test, derive(Debug, Clone, Eq, PartialEq, Copy))]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

#[cfg_attr(test, derive(Debug, Clone, Eq, PartialEq, Copy))]
pub enum BlockID {
    Hash(BlockHash),
    Number(BlockNumber),
}

impl Default for BlockID {
    fn default() -> Self {
        Self::Number(BlockNumber(0))
    }
}

// TODO: make this more generic to get more data types other then block
#[derive(Default)]
#[cfg_attr(test, derive(Debug, Clone, Eq, PartialEq, Copy))]
pub struct BlockQuery {
    pub start: BlockID,
    pub direction: Direction,
    pub limit: u64,
    pub skip: u64,
    pub step: u64,
    pub outbound_session_id: InboundSessionId,
}

#[derive(Default)]
pub struct BlockResult {
    pub outbound_session_id: InboundSessionId,
    pub data: Block,
}

// TODO(shahak): Implement conversion from GetBlocks to BlockQuery.
