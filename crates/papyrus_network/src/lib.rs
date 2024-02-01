/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod bin_utils;
pub mod block_headers;
mod db_executor;
pub mod messages;
pub mod network_manager;
pub mod streamed_data;
#[cfg(test)]
mod test_utils;

use std::time::Duration;

use starknet_api::block::BlockNumber;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct BlockQuery {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

// TODO: implement the SerializeConfig trait.
pub struct Config {
    pub listen_address: String,
    pub session_timeout: Duration,
    pub idle_connection_timeout: Duration,
    pub header_buffer_size: usize,
}
