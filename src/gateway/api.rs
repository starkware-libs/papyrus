use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;

use crate::starknet::BlockNumber;

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum JsonRpcError {
    #[error("There are no blocks.")]
    NoBlocks,
    #[error("Invalid block number")]
    InvalidBlockNumber = 26,
}

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    async fn block_number(&self) -> Result<BlockNumber, Error>;
}
