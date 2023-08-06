#[cfg(test)]
mod config_test;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::discriminant;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use clap::{arg, value_parser, Arg, ArgMatches, Command};
use itertools::chain;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::{append_sub_config_name, ser_optional_sub_config, SerializeConfig};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, SerializedParam};
use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/default_config.json";

/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct NodeConfig {
    pub gateway: GatewayConfig,
    pub central: CentralSourceConfig,
    pub base_layer: EthereumBaseLayerConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    /// None if the syncing should be disabled.
    pub sync: Option<SyncConfig>,
}

// Default configuration values.
impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            central: CentralSourceConfig::default(),
            base_layer: EthereumBaseLayerConfig::default(),
            gateway: GatewayConfig::default(),
            monitoring_gateway: MonitoringGatewayConfig::default(),
            storage: StorageConfig::default(),
            sync: Some(SyncConfig::default()),
        }
    }
}

impl SerializeConfig for NodeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        chain!(
            append_sub_config_name(self.central.dump(), "central"),
            append_sub_config_name(self.base_layer.dump(), "base_layer"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.monitoring_gateway.dump(), "monitoring_gateway"),
            append_sub_config_name(self.storage.dump(), "storage"),
            ser_optional_sub_config(&self.sync, "sync"),
        )
        .collect()
    }
}

impl NodeConfig {
    /// Creates a config object. Selects the values from the default file and from resources with
    /// higher priority.
    pub fn load_and_process(args: Vec<String>) -> Result<Self, ConfigError> {
        let path = Path::new(
            &env::var("CARGO_MANIFEST_DIR").expect("Env var 'CARGO_MANIFEST_DIR' did not found"),
        )
        .join("../..")
        .join(DEFAULT_CONFIG_PATH);
        let default_config_file = std::fs::File::open(path)
            .unwrap_or_else(|_| panic!("Failed to open file in {DEFAULT_CONFIG_PATH}"));
        load_and_process_config(default_config_file, node_command(), args)
    }

    pub fn get_config_representation(&self) -> Result<serde_json::Value, ConfigError> {
        Ok(serde_json::to_value(self)?)
    }
}

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Papyrus")
        .version(VERSION_FULL)
        .about("Papyrus is a StarkNet full node written in Rust.")
}
