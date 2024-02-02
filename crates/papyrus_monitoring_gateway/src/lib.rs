// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(test)]
mod gateway_test;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::{BuildError, PrometheusBuilder, PrometheusHandle};
use metrics_process::Collector;
use papyrus_config::converters::{deserialize_optional_map, serialize_optional_map};
use papyrus_config::dumping::{ser_generated_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use papyrus_storage::{DbStats, StorageError, StorageReader};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};
use starknet_client::writer::{StarknetGatewayClient, StarknetWriter};
use starknet_client::RetryConfig;
use tracing::{debug, info, instrument};
use validator::Validate;

const MONITORING_PREFIX: &str = "monitoring";
const PROCESS_METRICS_PREFIX: &str = "papyrus_";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct MonitoringGatewayConfig {
    pub server_address: String,
    pub collect_metrics: bool,
    #[serde(deserialize_with = "deserialize_optional_map")]
    pub metric_labels: Option<HashMap<String, String>>,
    #[validate(length(min = 1))]
    #[serde(default = "random_secret")]
    pub present_full_config_secret: String,
    pub starknet_url: String,
}

fn random_secret() -> String {
    let secret = thread_rng().sample_iter(&Alphanumeric).take(10).map(char::from).collect();
    info!("The randomly generated config presentation secret is: {}", secret);
    secret
}

impl Default for MonitoringGatewayConfig {
    fn default() -> Self {
        MonitoringGatewayConfig {
            server_address: String::from("0.0.0.0:8081"),
            collect_metrics: false,
            metric_labels: None,
            // A constant value for testing purposes.
            present_full_config_secret: String::from("qwerty"),
            starknet_url: String::from("https://alpha-mainnet.starknet.io/"),
        }
    }
}

impl SerializeConfig for MonitoringGatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "server_address",
                &self.server_address,
                "node's monitoring server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_metrics",
                &self.collect_metrics,
                "If true, collect and return metrics in the monitoring gateway.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "metric_labels",
                &serialize_optional_map(&self.metric_labels),
                "'label1:value1 label2:value2 ...' additional labels for metrics.",
                ParamPrivacyInput::Public,
            ),
            ser_generated_param(
                "present_full_config_secret",
                SerializationType::String,
                "A secret for presenting the full general config.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "starknet_url",
                &self.starknet_url,
                "The URL of a centralized Starknet gateway.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Display for MonitoringGatewayConfig {
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub struct MonitoringServer {
    config: MonitoringGatewayConfig,
    // Nested Json presentation of all the parameters in the node config.
    full_general_config_presentation: serde_json::Value,
    // Nested Json presentation of the public parameters in the node config.
    public_general_config_presentation: serde_json::Value,
    storage_reader: StorageReader,
    version: &'static str,
    prometheus_handle: Option<PrometheusHandle>,
}

impl MonitoringServer {
    pub fn new(
        config: MonitoringGatewayConfig,
        full_general_config_presentation: serde_json::Value,
        public_general_config_presentation: serde_json::Value,
        storage_reader: StorageReader,
        version: &'static str,
    ) -> Result<Self, BuildError> {
        let prometheus_handle = if config.collect_metrics {
            let mut builder = PrometheusBuilder::new();
            if let Some(metric_labels) = &config.metric_labels {
                for (label, value) in metric_labels {
                    builder = builder.add_global_label(label, value);
                }
            }
            Some(builder.install_recorder()?)
        } else {
            None
        };
        Ok(MonitoringServer {
            config,
            storage_reader,
            full_general_config_presentation,
            public_general_config_presentation,
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
            full_general_config_presentation = %self.full_general_config_presentation,
            public_general_config_presentation = %self.public_general_config_presentation,
            present_full_config_secret = %self.config.present_full_config_secret),
        level = "debug")]
    async fn run_server(&self) -> std::result::Result<(), hyper::Error> {
        let server_address = SocketAddr::from_str(&self.config.server_address)
            .expect("Configuration value for monitor server address should be valid");
        let app = app(
            self.config.starknet_url.clone(),
            self.storage_reader.clone(),
            self.version,
            self.full_general_config_presentation.clone(),
            self.public_general_config_presentation.clone(),
            self.config.present_full_config_secret.clone(),
            self.prometheus_handle.clone(),
        );
        debug!("Starting monitoring gateway.");
        axum::Server::bind(&server_address).serve(app.into_make_service()).await
    }
}

fn app(
    starknet_url: String,
    storage_reader: StorageReader,
    version: &'static str,
    full_general_config_presentation: serde_json::Value,
    public_general_config_presentation: serde_json::Value,
    present_full_config_secret: String,
    prometheus_handle: Option<PrometheusHandle>,
) -> Router {
    let is_ready_retry_config =
        RetryConfig { retry_base_millis: 50, retry_max_delay_millis: 1000, max_retries: 0 };
    let starknet_feeder_client = Arc::new(
        StarknetFeederGatewayClient::new(
            starknet_url.as_str(),
            None,
            version,
            is_ready_retry_config,
        )
        .expect("Failed creating Starknet feeder client."),
    );
    let starknet_client = Arc::new(
        StarknetGatewayClient::new(starknet_url.as_str(), version, is_ready_retry_config)
            .expect("Failed creating Starknet client."),
    );

    Router::new()
        .route(
            format!("/{MONITORING_PREFIX}/dbTablesStats").as_str(),
            get(move || db_tables_stats(storage_reader)),
        )
        .route(
            format!("/{MONITORING_PREFIX}/nodeConfig").as_str(),
            get(move || node_config(public_general_config_presentation)),
        )
        .route(
            // The "*secret" captures the end of the path and stores it in "secret".
            format!("/{MONITORING_PREFIX}/nodeConfigFull/*secret").as_str(),
            get(move |secret| {
                node_config_by_secret(
                    full_general_config_presentation,
                    secret,
                    present_full_config_secret,
                )
            }),
        )
        .route(
            format!("/{MONITORING_PREFIX}/nodeVersion").as_str(),
            get(move || node_version(version)),
        )
        .route(
            format!("/{MONITORING_PREFIX}/alive").as_str(),
            get(move || async { StatusCode::OK.to_string() }),
        )
        .route(
            format!("/{MONITORING_PREFIX}/metrics").as_str(),
            get(move || metrics(prometheus_handle)),
        )
        .route(
            format!("/{MONITORING_PREFIX}/ready").as_str(),
            get(move || is_ready(starknet_client, starknet_feeder_client)),
        )
}

async fn is_ready<TStarknetWriter: StarknetWriter, TStarknetReader: StarknetReader>(
    starknet_client: Arc<TStarknetWriter>,
    starknet_feeder_client: Arc<TStarknetReader>,
) -> String {
    let response = starknet_feeder_client.is_alive().await;
    assert!(response);

    let response = starknet_client.is_alive().await;
    assert!(response);

    StatusCode::OK.to_string()
}

/// Returns DB statistics.
#[instrument(skip(storage_reader), level = "debug", ret)]
async fn db_tables_stats(storage_reader: StorageReader) -> Result<Json<DbStats>, ServerError> {
    Ok(storage_reader.db_tables_stats()?.into())
}

/// Returns the node config.
#[instrument(level = "debug", ret)]
async fn node_config(
    full_general_config_presentation: serde_json::Value,
) -> axum::Json<serde_json::Value> {
    full_general_config_presentation.into()
}

/// Returns the node config.
#[instrument(level = "debug", ret)]
async fn node_config_by_secret(
    full_general_config_presentation: serde_json::Value,
    given_secret: Path<String>,
    expected_secret: String,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    if given_secret.to_string() == expected_secret {
        Ok(node_config(full_general_config_presentation).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Returns prometheus metrics.
/// In case the node doesn’t collect metrics returns an empty response with status code 405: method
/// not allowed.
#[instrument(level = "debug", ret, skip(prometheus_handle))]
async fn metrics(prometheus_handle: Option<PrometheusHandle>) -> Response {
    match prometheus_handle {
        Some(handle) => {
            Collector::new(PROCESS_METRICS_PREFIX).collect();
            handle.render().into_response()
        }
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
