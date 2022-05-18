mod api;

use std::net::SocketAddr;

use jsonrpsee::{
    core::{async_trait, Error},
    ws_server::{WsServerBuilder, WsServerHandle},
};

use crate::starknet::BlockNumber;

use self::api::RpcServer;

/// Rpc server.
struct RpcServerImpl;

#[async_trait]
impl RpcServer for RpcServerImpl {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        Ok(BlockNumber(0))
    }
}

#[allow(dead_code)]
async fn run_server() -> anyhow::Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::default().build("127.0.0.1:0").await?;
    let addr = server.local_addr()?;
    let handle = server.start(RpcServerImpl.into_rpc())?;
    Ok((addr, handle))
}

#[cfg(test)]
mod tests {
    use crate::{
        gateway::{api::RpcServer, RpcServerImpl},
        starknet::BlockNumber,
    };
    use jsonrpsee::types::EmptyParams;

    #[tokio::test]
    async fn get_block_number() {
        let module = RpcServerImpl.into_rpc();
        let result: BlockNumber = module
            .call("starknet_blockNumber", EmptyParams::new())
            .await
            .unwrap();
        assert_eq!(result, BlockNumber(0));
    }
}
