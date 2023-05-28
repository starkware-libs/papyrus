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
use prometheus::{Encoder, Registry};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

const MONITORING_PREFIX: &str = "monitoring";

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
    gw_prometheus_registry: Registry,
    version: &'static str,
}

impl MonitoringServer {
    pub fn new(
        config: MonitoringGatewayConfig,
        general_config_representation: serde_json::Value,
        storage_reader: StorageReader,
        gw_prometheus_registry: Registry,
        version: &'static str,
    ) -> Self {
        MonitoringServer {
            gw_prometheus_registry,
            config,
            storage_reader,
            general_config_representation,
            version,
        }
    }

    /// Spawns a monitoring server.
    pub async fn spawn_server(self) -> tokio::task::JoinHandle<Result<(), hyper::Error>> {
        tokio::spawn(async move { self.run_server().await })
    }

    #[instrument(
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
            self.gw_prometheus_registry.clone(),
        );
        debug!("Starting monitoring gateway.");
        axum::Server::bind(&server_address).serve(app.into_make_service()).await
    }
}

fn app(
    storage_reader: StorageReader,
    version: &'static str,
    general_config_representation: serde_json::Value,
    gw_prometheus_registry: Registry,
) -> Router {
    Router::new()
        .route(
            format!("/{MONITORING_PREFIX}/dbTablesStats").as_str(),
            get(move || db_tables_stats(storage_reader)),
        )
        .route(
            format!("/{MONITORING_PREFIX}/nodeConfig").as_str(),
            get(move || node_config(general_config_representation)),
        )
        .route(
            format!("/{MONITORING_PREFIX}/nodeVersion").as_str(),
            get(move || node_version(version)),
        )
        .route(
            format!("/{MONITORING_PREFIX}/alive").as_str(),
            get(move || async { StatusCode::OK }),
        )
        .route(
            format!("/{MONITORING_PREFIX}/metrics").as_str(),
            get(move || get_metrics(gw_prometheus_registry)),
        )
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

// Returns prometheus metrics.
#[instrument(level = "debug", ret)]
async fn get_metrics(gateway_registry: Registry) -> Result<Vec<u8>, ServerError> {
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&gateway_registry.gather(), &mut buffer)?;
    // Metrics of DEFAULT_REGISTRY, include time and memory usage.
    // Those metrics are not include in mac os.
    if std::env::consts::OS != "macos" {
        encoder.encode(&prometheus::gather(), &mut buffer)?;
    }
    Ok(buffer)
}

#[derive(thiserror::Error, Debug)]
enum ServerError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    PrometheusMetricError(#[from] prometheus::Error),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            // TODO(dan): consider using a generic error message instead.
            ServerError::StorageError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            ServerError::PrometheusMetricError(err) => {
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
        };
        (status, error_message).into_response()
    }
}
