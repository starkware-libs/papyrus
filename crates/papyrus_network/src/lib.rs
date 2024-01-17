/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod bin_utils;
pub mod block_headers;
mod db_executor;
pub mod messages;
pub mod streamed_data;
#[cfg(test)]
mod test_utils;

use libp2p::swarm::NetworkBehaviour;
use starknet_api::block::BlockNumber;
use streamed_data::Config;

#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Direction {
    Forward,
    Backward,
}

#[cfg_attr(test, derive(Debug))]
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct BlockQuery {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

pub trait PapyrusBehaviour: NetworkBehaviour {
    // TODO: create a generic network config and use that instead of the streamed data one.
    fn new(config: Config) -> Self
    where
        Self: Sized;
}
