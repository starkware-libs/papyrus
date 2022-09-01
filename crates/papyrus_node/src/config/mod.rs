#[cfg(test)]
mod config_test;

use std::fs;

use papyrus_gateway::GatewayConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub central: CentralSourceConfig,
    pub gateway: GatewayConfig,
    pub storage: StorageConfig,
    pub sync: SyncConfig,
}

pub fn load_config(path: &str) -> anyhow::Result<Config> {
    let config_contents = fs::read_to_string(path).expect("Something went wrong reading the file");
    let config: Config = ron::from_str(&config_contents)?;
    Ok(config)
}
