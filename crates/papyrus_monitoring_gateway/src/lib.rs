#[cfg(test)]
mod gateway_test;

use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use papyrus_storage::{DbTablesStats, StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitoringGatewayConfig {
    pub server_address: String,
}

impl Display for MonitoringGatewayConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub struct MonitoringServer {
    config: MonitoringGatewayConfig,
    general_config_representation: serde_json::Value,
    storage_reader: StorageReader,
    version: &'static str,
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

    /// Spawns a monitoring server.
    pub async fn spawn_server(self) -> tokio::task::JoinHandle<Result<(), hyper::Error>> {
        tokio::spawn(async move { self.run_server().await })
    }

    #[instrument(
        name = "run monitoring server",
        skip(self),
        fields(
            version = %self.version,
            config = %self.config,
            general_config_representation = %self.general_config_representation),
        level = "debug")]
    async fn run_server(&self) -> std::result::Result<(), hyper::Error> {
        let server_address = SocketAddr::from_str(&self.config.server_address)
            .expect("Configuration value for monitor server address should be valid");
        let app = app(
            self.storage_reader.clone(),
            self.version,
            self.general_config_representation.clone(),
        );
        debug!("Starting monitoring gateway.");
        axum::Server::bind(&server_address).serve(app.into_make_service()).await
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

#[derive(thiserror::Error, Debug)]
enum ServerError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
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
