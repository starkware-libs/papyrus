use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;

use crate::starknet::BlockNumber;

#[rpc(server, client, namespace = "starknet")]
pub trait Rpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    async fn block_number(&self) -> Result<BlockNumber, Error>;
}
