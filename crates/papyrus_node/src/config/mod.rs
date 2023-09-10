#[cfg(test)]
mod config_test;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::discriminant;
use std::ops::IndexMut;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use clap::{arg, value_parser, Arg, ArgMatches, Command};
use itertools::{chain, Itertools};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::{append_sub_config_name, ser_optional_sub_config, SerializeConfig};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, SerializedParam};
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_rpc::RpcConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::sources::central::CentralSourceConfig;
use papyrus_sync::SyncConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;
use validator::Validate;

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
pub const DEFAULT_CONFIG_PATH: &str = "config/default_config.json";

/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct NodeConfig {
    #[validate]
    pub rpc: RpcConfig,
    pub central: CentralSourceConfig,
    pub base_layer: EthereumBaseLayerConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    #[validate]
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
            rpc: RpcConfig::default(),
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
            append_sub_config_name(self.rpc.dump(), "rpc"),
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
        let default_config_file = std::fs::File::open(Path::new(DEFAULT_CONFIG_PATH))?;
        load_and_process_config(default_config_file, node_command(), args)
    }
}

/// The command line interface of this node.
pub fn node_command() -> Command {
    Command::new("Papyrus")
        .version(VERSION_FULL)
        .about("Papyrus is a StarkNet full node written in Rust.")
}
