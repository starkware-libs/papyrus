#[cfg(test)]
mod config_test;

use std::fs;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub server_ip: String,
}

#[derive(Serialize, Deserialize)]
pub struct MonitoringGatewayConfig {
    pub server_ip: String,
}

#[derive(Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_config: DbConfig,
}

#[derive(Serialize, Deserialize)]
pub struct DbConfig {
    pub path: String,
    pub max_size: usize,
}

#[derive(Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub url: String,
    pub retry_config: RetryConfig,
}

/// A configuration for the retry mechanism.
#[derive(Serialize, Deserialize)]
pub struct RetryConfig {
    /// The initial waiting time in milliseconds.
    pub retry_base_millis: u64,
    /// The maximum waiting time in milliseconds.
    pub retry_max_delay_millis: u64,
    /// The maximum number of retries.
    pub max_retries: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SyncConfig {
    pub block_propagation_sleep_duration: Duration,
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub central: CentralSourceConfig,
    pub gateway: GatewayConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    pub sync: SyncConfig,
}

pub fn load_config(path: &'static str) -> anyhow::Result<Config> {
    let config_contents = fs::read_to_string(path).expect("Something went wrong reading the file");
    let config: Config = ron::from_str(&config_contents)?;
    Ok(config)
}
