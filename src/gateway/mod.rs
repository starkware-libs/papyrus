mod api;
#[cfg(test)]
mod gateway_test;

use std::fmt::Display;
use std::net::SocketAddr;

use jsonrpsee::core::{async_trait, Error};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::{ErrorObject, INTERNAL_ERROR_MSG};
use jsonrpsee::ws_server::types::error::CallError;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use log::error;

use crate::starknet::BlockNumber;
use crate::storage::components::BlockStorageReader;

use api::JsonRpcError;
use api::JsonRpcServer;

// TODO(anatg): Take from config.
const SERVER_IP: &str = "127.0.0.1:0";

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: BlockStorageReader,
}

impl From<JsonRpcError> for Error {
    fn from(err: JsonRpcError) -> Self {
        Error::Call(CallError::Custom(ErrorObject::owned(
            err as i32,
            err.to_string(),
            None::<()>,
        )))
    }
}

fn internal_server_error(err: impl Display) -> Error {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    Error::Call(CallError::Custom(ErrorObject::owned(
        InternalError.code(),
        INTERNAL_ERROR_MSG,
        None::<()>,
    )))
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        self.storage_reader
            .get_header_marker()
            .map_err(internal_server_error)?
            .prev()
            .ok_or_else(|| JsonRpcError::NoBlocks.into())
    }
}

pub async fn run_server(
    storage_reader: BlockStorageReader,
) -> anyhow::Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::default().build(SERVER_IP).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    Ok((addr, handle))
}
