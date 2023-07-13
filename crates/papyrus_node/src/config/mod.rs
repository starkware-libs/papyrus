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
use papyrus_config::{
    append_sub_config_name, load_and_process_config, ParamPath, SerializeConfig, SerializedParam,
    SubConfigError, DEFAULT_CHAIN_ID,
};
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
pub const DEFAULT_CONFIG_PATH: &str = "crates/papyrus_node/src/config/default_config.json";

// TODO(yoav) Rename to NodeConfig.
/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Config {
    pub gateway: GatewayConfig,
    pub central: CentralSourceConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    /// None if the syncing should be disabled.
    pub sync: Option<SyncConfig>,
}

// Default configuration values.
impl Default for Config {
    fn default() -> Self {
        Config {
            central: CentralSourceConfig::default(),
            gateway: GatewayConfig::default(),
            monitoring_gateway: MonitoringGatewayConfig::default(),
            storage: StorageConfig::default(),
            sync: Some(SyncConfig::default()),
        }
    }
}

impl SerializeConfig for Config {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        chain!(
            append_sub_config_name(self.central.dump(), "central"),
            append_sub_config_name(self.gateway.dump(), "gateway"),
            append_sub_config_name(self.monitoring_gateway.dump(), "monitoring_gateway"),
            append_sub_config_name(self.storage.dump(), "storage"),
            match self.sync {
                None => BTreeMap::new(),
                Some(sync_config) => append_sub_config_name(sync_config.dump(), "sync"),
            },
        )
        .collect()
    }
}

pub fn dump_default_config_to_file(file_path: &str) {
    let dumped = Config::default().dump();
    let file = File::create(file_path).expect("creating failed");
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &dumped).expect("writing failed");
    writer.flush().expect("flushing failed");
}

pub fn node_command() -> Command {
    Command::new("Papyrus")
        .version(VERSION_FULL)
        .about("Papyrus is a StarkNet full node written in Rust.")
}

impl Config {
    pub fn load_and_process(args: Vec<String>) -> Result<Self, SubConfigError> {
        let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../..")
            .join(DEFAULT_CONFIG_PATH);
        let default_config_file = std::fs::File::open(path).unwrap();
        load_and_process_config(default_config_file, node_command(), args)
    }

    pub fn get_config_representation(&self) -> Result<serde_json::Value, SubConfigError> {
        Ok(serde_json::to_value(self)?)
    }
}
