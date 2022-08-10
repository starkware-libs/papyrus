mod api;
#[cfg(test)]
mod gateway_test;

use std::fmt::Display;
use std::net::SocketAddr;

// use api::JsonRpcError;
use jsonrpsee::core::{async_trait, Error};
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::http_server::{HttpServerBuilder, HttpServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::{ErrorObject, INTERNAL_ERROR_MSG};
use log::{error, info};
use papyrus_storage::{DbTablesStats, StorageReader};
use serde::{Deserialize, Serialize};

use self::api::JsonRpcServer;

#[derive(Serialize, Deserialize)]
pub struct MonitoringGatewayConfig {
    pub server_ip: String,
}

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: StorageReader,
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
    fn db_tables_stats(&self) -> Result<DbTablesStats, Error> {
        self.storage_reader.db_tables_stats().map_err(internal_server_error)
    }
}

pub async fn run_server(
    config: MonitoringGatewayConfig,
    storage_reader: StorageReader,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    info!("Starting monitoring gateway.");
    let server = HttpServerBuilder::default().build(&config.server_ip).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    info!("Monitoring gateway is running - {}.", addr);
    Ok((addr, handle))
}
