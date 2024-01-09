/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub(crate) mod block_headers;
mod db_executor;
pub mod messages;
pub mod streamed_data;
#[cfg(test)]
mod test_utils;

use starknet_api::block::BlockNumber;

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub enum Direction {
    Forward,
    Backward,
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct BlockQuery {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}
