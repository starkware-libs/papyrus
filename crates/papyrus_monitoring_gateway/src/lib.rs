#[cfg(test)]
mod gateway_test;

use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use papyrus_storage::{DbTablesStats, StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MonitoringGatewayConfig {
    pub server_address: String,
}

pub struct MonitoringServer {
    pub config: MonitoringGatewayConfig,
    pub general_config_representation: serde_json::Value,
    pub storage_reader: StorageReader,
    pub version: &'static str,
}

impl MonitoringServer {
    pub fn new(
        config: MonitoringGatewayConfig,
        general_config_representation: serde_json::Value,
        storage_reader: StorageReader,
        version: &'static str,
    ) -> Self {
        MonitoringServer { config, storage_reader, general_config_representation, version }
    }

    pub async fn run_server(
        &self,
    ) -> tokio::task::JoinHandle<std::result::Result<(), hyper::Error>> {
        let server_address = SocketAddr::from_str(&self.config.server_address)
            .expect("Valid configuration value for monitor server address");
        let app = app(
            self.storage_reader.clone(),
            self.version,
            self.general_config_representation.clone(),
        );
        info!("Starting monitoring gateway, listening on {server_address:}.");
        tokio::spawn(async move {
            axum::Server::bind(&server_address).serve(app.into_make_service()).await
        })
    }
}

fn app(
    storage_reader: StorageReader,
    version: &'static str,
    general_config_representation: serde_json::Value,
) -> Router {
    Router::new()
        .route("/dbTablesStats", get(move || db_tables_stats(storage_reader)))
        .route("/nodeConfig", get(move || node_config(general_config_representation)))
        .route("/nodeVersion", get(move || node_version(version)))
}

/// Returns DB statistics.
#[instrument(skip(storage_reader), level = "debug", ret)]
async fn db_tables_stats(
    storage_reader: StorageReader,
) -> Result<Json<DbTablesStats>, ServerError> {
    Ok(storage_reader.db_tables_stats()?.into())
}

/// Returns the node config.
#[instrument(level = "debug", ret)]
async fn node_config(
    general_config_representation: serde_json::Value,
) -> axum::Json<serde_json::Value> {
    general_config_representation.into()
}

/// Returns the node version.
#[instrument(level = "debug", ret)]
async fn node_version(version: &'static str) -> String {
    version.to_string()
}

#[derive(Debug)]
enum ServerError {
    StorageError(StorageError),
}

impl From<StorageError> for ServerError {
    fn from(inner: StorageError) -> Self {
        ServerError::StorageError(inner)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            // TODO(dan): consider using a generic error message instead.
            ServerError::StorageError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        (status, error_message).into_response()
    }
}
