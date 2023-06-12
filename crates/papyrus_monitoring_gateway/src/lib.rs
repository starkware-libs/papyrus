//! Monitoring gateway for [`starknet`] papyrus node.
//!
//! [`starknet`]: https://starknet.io/

#[cfg(test)]
mod gateway_test;

use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::{BuildError, PrometheusBuilder, PrometheusHandle};
use papyrus_storage::{DbTablesStats, StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

const MONITORING_PREFIX: &str = "monitoring";
// TODO(dvir): Add to config.
const COLLECT_METRICS: bool = true;

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
    prometheus_handle: Option<PrometheusHandle>,
}

impl MonitoringServer {
    pub fn new(
        config: MonitoringGatewayConfig,
        general_config_representation: serde_json::Value,
        storage_reader: StorageReader,
        version: &'static str,
    ) -> Result<Self, BuildError> {
        let prometheus_handle =
            if COLLECT_METRICS { Some(PrometheusBuilder::new().install_recorder()?) } else { None };
        Ok(MonitoringServer {
            config,
            storage_reader,
            general_config_representation,
            version,
            prometheus_handle,
        })
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
            self.prometheus_handle.clone(),
        );
        debug!("Starting monitoring gateway.");
        axum::Server::bind(&server_address).serve(app.into_make_service()).await
    }
}

fn app(
    storage_reader: StorageReader,
    version: &'static str,
    general_config_representation: serde_json::Value,
    prometheus_handle: Option<PrometheusHandle>,
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
            get(move || metrics(prometheus_handle)),
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

/// Returns prometheus metrics.
/// In case the node doesn’t collect metrics returns an empty response with status code 405: method
/// not allowed.
#[instrument(level = "debug", ret, skip(prometheus_handle))]
async fn metrics(prometheus_handle: Option<PrometheusHandle>) -> Response {
    match prometheus_handle {
        Some(handle) => handle.render().into_response(),
        None => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
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
