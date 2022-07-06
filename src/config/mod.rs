#[cfg(test)]
mod config_test;

use std::fs;

use serde::{Deserialize, Serialize};

use crate::gateway::GatewayConfig;
use crate::storage::components::StorageConfig;
use crate::sync::{CentralSourceConfig, SyncConfig};

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub storage: StorageConfig,
    pub central: CentralSourceConfig,
    pub sync: SyncConfig,
    pub gateway: GatewayConfig,
}

pub fn load_config(path: &'static str) -> anyhow::Result<Config> {
    let config_contents = fs::read_to_string(path).expect("Something went wrong reading the file");
    let config: Config = ron::from_str(&config_contents)?;
    Ok(config)
}
