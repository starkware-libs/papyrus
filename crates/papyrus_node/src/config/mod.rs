#[cfg(test)]
mod config_test;

use std::fs;

use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::ChainId;

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub chain_id: ChainId,
    pub gateway: (String, usize, usize),
    pub central: CentralSourceConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    pub sync: SyncConfig,
}

pub fn load_config(path: &'static str) -> anyhow::Result<Config> {
    let config_contents = fs::read_to_string(path).expect("Something went wrong reading the file");
    let config: Config = ron::from_str(&config_contents)?;
    Ok(config)
}

impl Config {
    pub fn get_gateway_config(&self) -> GatewayConfig {
        GatewayConfig {
            chain_id: self.chain_id.clone(),
            server_ip: self.gateway.0.clone(),
            max_events_chunk_size: self.gateway.1,
            max_events_keys: self.gateway.2,
        }
    }
}
