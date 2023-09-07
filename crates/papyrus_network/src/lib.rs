pub mod db_executor;
/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod get_blocks;
pub mod messages;

use starknet_api::block::{BlockHash, BlockNumber};

pub enum Direction {
    Forward,
    Backward,
}

pub enum BlockID {
    Hash(BlockHash),
    Number(BlockNumber),
}

pub struct BlocksRange {
    pub start: BlockID,
    pub direction: Direction,
    pub limit: u64,
    pub skip: u64,
    pub step: u64,
}

// TODO(shahak): Implement conversion from GetBlocks to BlocksRange.
