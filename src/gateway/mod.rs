mod api;
#[cfg(test)]
mod gateway_test;

use std::{fmt::Display, net::SocketAddr};

use jsonrpsee::{
    core::{async_trait, Error},
    types::error::{ErrorCode::InternalError, ErrorObject, INTERNAL_ERROR_MSG},
    ws_server::{types::error::CallError, WsServerBuilder, WsServerHandle},
};

use crate::{
    starknet::BlockNumber,
    storage::{SNStorageReader, StorageReader},
};

use self::api::JsonRpcServer;

// TODO(anatg): Take from config.
const SERVER_IP: &str = "127.0.0.1:0";

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: SNStorageReader,
}

fn internal_server_error(err: impl Display) -> Error {
    Error::Call(CallError::Custom(ErrorObject::owned(
        InternalError.code(),
        format!("{}: {}", INTERNAL_ERROR_MSG, err),
        None::<()>,
    )))
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        Ok(self
            .storage_reader
            .get_latest_block_number()
            .await
            .map_err(internal_server_error)?)
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
