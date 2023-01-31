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
use papyrus_storage::{DbTablesStats, StorageReader};
use serde::{Deserialize, Serialize};
use tracing::{debug, debug_span, error, info, instrument, Span};

use self::api::JsonRpcServer;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MonitoringGatewayConfig {
    pub server_address: String,
}

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: StorageReader,
    server_span: Span,
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
    #[instrument(skip(self), parent = &self.server_span, level = "debug", err(Display), ret)]
    fn db_tables_stats(&self) -> Result<DbTablesStats, Error> {
        self.storage_reader.db_tables_stats().map_err(internal_server_error)
    }
}

pub async fn run_server(
    config: MonitoringGatewayConfig,
    storage_reader: StorageReader,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    debug!("Starting monitoring gateway.");
    let server = HttpServerBuilder::default().build(&config.server_address).await?;
    let addr = server.local_addr()?;
    let handle = server.start(
        JsonRpcServerImpl { storage_reader, server_span: debug_span!("monitoring_gateway_run", ?config) }
            .into_rpc(),
    )?;
    info!(local_address = %addr, "Monitoring gateway is running.");
    Ok((addr, handle))
}
