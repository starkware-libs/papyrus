mod api;
#[cfg(test)]
mod gateway_test;

use std::net::SocketAddr;

use anyhow::anyhow;
use jsonrpsee::{
    core::{async_trait, Error},
    ws_server::{types::error::CallError, WsServerBuilder, WsServerHandle},
};
use log::error;

use crate::{
    starknet::BlockNumber,
    storage::{SNStorageReader, StorageError, StorageReader},
};

use self::api::JsonRpcServer;

// TODO(anatg): Take from config.
const SERVER_IP: &str = "127.0.0.1:0";

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: SNStorageReader,
}

impl From<StorageError> for Error {
    fn from(error: StorageError) -> Self {
        error!("Storage error: {:?}", error);
        Error::Call(CallError::Failed(anyhow!("Internal server error.")))
    }
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        Ok(self.storage_reader.get_latest_block_number().await?)
    }
}

#[allow(dead_code)]
pub async fn run_server(
    storage_reader: SNStorageReader,
) -> anyhow::Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::default().build(SERVER_IP).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    Ok((addr, handle))
}
