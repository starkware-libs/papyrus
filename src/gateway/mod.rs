mod api;
#[cfg(test)]
mod gateway_test;

use std::net::SocketAddr;

use jsonrpsee::{
    core::{async_trait, Error},
    ws_server::{WsServerBuilder, WsServerHandle},
};
use log::{error, info};

use crate::{starknet::BlockNumber, storage::StorageReader};

use self::api::JsonRpcServer;

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: Box<dyn StorageReader>,
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        let res = self.storage_reader.get_latest_block_number().await;
        match res {
            Ok(block_number) => {
                info!("Read block number: {:?}.", block_number);
                Ok(block_number)
            }
            err => {
                error!("Storage error: {:?}", err);
                Err(Error::Custom(format!("Storage error: {:?}", err)))
            }
        }
    }
}

#[allow(dead_code)]
pub async fn run_server(
    storage_reader: Box<dyn StorageReader>,
) -> anyhow::Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::default().build("127.0.0.1:0").await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    Ok((addr, handle))
}
