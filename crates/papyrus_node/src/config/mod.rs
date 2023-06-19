#[cfg(test)]
mod config_test;

mod file_config;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

use clap::{arg, value_parser, Arg, ArgMatches, Command};
use file_config::FileConfigFormat;
use itertools::chain;
use papyrus_config::{
    append_sub_config_name, ParamPath, SerializeConfig, SerializedParam, DEFAULT_CHAIN_ID,
};
use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;

use crate::version::VERSION_FULL;

// The path of the default configuration file, provided as part of the crate.
const CONFIG_FILE: &str = "config/config.yaml";

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
            // TODO(yoav): Read the default values from a dump file.
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
    pub fn load_from_builder(args: Vec<String>) -> Result<Self, ConfigError> {
        ConfigBuilder::build(args)
    }

    pub fn get_config_representation(&self) -> Result<serde_json::Value, ConfigError> {
        Ok(serde_json::to_value(FileConfigFormat::from(self.clone()))?)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Unable to parse path: {path}")]
    BadPath { path: PathBuf },
    #[error(transparent)]
    Clap(#[from] clap::Error),
    #[error(transparent)]
    Matches(#[from] clap::parser::MatchesError),
    #[error(transparent)]
    Read(#[from] io::Error),
    #[error(transparent)]
    Serde(#[from] serde_yaml::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(
        "CLA http_header \"{illegal_header}\" is not valid. The Expected format is name:value"
    )]
    CLAHttpHeader { illegal_header: String },
}

// Builds the configuration for the node based on default values, yaml configuration file and
// command-line arguments.
// TODO: add configuration from env variables.
pub(crate) struct ConfigBuilder {
    args: Option<ArgMatches>,
    chain_id: ChainId,
    config: Config,
}

// Default configuration values.
impl Default for ConfigBuilder {
    fn default() -> Self {
        ConfigBuilder {
            args: None,
            chain_id: ChainId(DEFAULT_CHAIN_ID.to_string()),
            config: Config::default(),
        }
    }
}

impl ConfigBuilder {
    // Creates the configuration struct.
    fn build(args: Vec<String>) -> Result<Config, ConfigError> {
        Ok(Self::default().prepare_command(args)?.yaml()?.args()?.propagate_chain_id().config)
    }

    // Builds the applications command-line interface.
    fn prepare_command(mut self, args: Vec<String>) -> Result<Self, ConfigError> {
        self.args = Some(
            node_command()
            .args(&[
                arg!(-f --config_file [path] "Optionally sets a config file to use").value_parser(value_parser!(PathBuf)),
                arg!(-c --chain_id [name] "Optionally sets chain id to use"),
                arg!(--server_address ["IP:PORT"] "Optionally sets the RPC listening address"),
                arg!(--http_headers ["NAME:VALUE"] ... "Optionally adds headers to the http requests"),
                arg!(-s --storage [path] "Optionally sets storage path to use (automatically extended with chain ID)").value_parser(value_parser!(PathBuf)),
                arg!(-n --no_sync [bool] "Optionally run without sync").value_parser(value_parser!(bool)).default_missing_value("true"),
                arg!(--central_url ["URL"] "Central URL. It should match chain_id."),
                arg!(--collect_metrics [bool] "Collect metrics for the node").value_parser(value_parser!(bool)).default_missing_value("true"),
            ])
            .try_get_matches_from(args).unwrap_or_else(|e| e.exit()),
        );
        Ok(self)
    }

    // Parses a yaml configuration file given by the command-line args (or default), and applies it
    // on the configuration.
    fn yaml(mut self) -> Result<Self, ConfigError> {
        let mut yaml_path = CONFIG_FILE;

        let args = self.args.clone().expect("Config builder should have args.");
        if let Some(config_file) = args.try_get_one::<PathBuf>("config_file")? {
            yaml_path =
                config_file.to_str().ok_or(ConfigError::BadPath { path: config_file.clone() })?;
        }

        let yaml_contents = fs::read_to_string(yaml_path)?;
        let from_yaml: FileConfigFormat = serde_yaml::from_str(&yaml_contents)?;
        from_yaml.update_config(&mut self);

        Ok(self)
    }

    // Reads the command-line args and updates the relevant configurations.
    fn args(mut self) -> Result<Self, ConfigError> {
        match self.args {
            None => unreachable!(),
            Some(ref args) => {
                if let Some(chain_id) = args.try_get_one::<String>("chain_id")? {
                    self.chain_id = ChainId(chain_id.clone());
                }

                if let Some(server_address) = args.try_get_one::<String>("server_address")? {
                    self.config.gateway.server_address = server_address.to_string()
                }

                if let Some(storage_path) = args.try_get_one::<PathBuf>("storage")? {
                    self.config.storage.db_config.path = storage_path.to_owned();
                }

                if let Some(http_headers) = args.try_get_one::<String>("http_headers")? {
                    let mut headers_map = match self.config.central.http_headers {
                        Some(map) => map,
                        None => HashMap::new(),
                    };
                    for header in http_headers.split(' ') {
                        let split: Vec<&str> = header.split(':').collect();
                        if split.len() != 2 {
                            return Err(ConfigError::CLAHttpHeader {
                                illegal_header: header.to_string(),
                            });
                        }
                        headers_map.insert(split[0].to_string(), split[1].to_string());
                    }
                    self.config.central.http_headers = Some(headers_map);
                }

                if let Some(no_sync) = args.try_get_one::<bool>("no_sync")? {
                    if *no_sync {
                        self.config.sync = None;
                    }
                }
                if let Some(central_url) = args.try_get_one::<String>("central_url")? {
                    self.config.central.url = central_url.to_string()
                }
                if let Some(collect_metrics) = args.try_get_one::<bool>("collect_metrics")? {
                    self.config.monitoring_gateway.collect_metrics = *collect_metrics;
                }

                Ok(self)
            }
        }
    }

    // Propagates the chain id into all the of configurations that use it.
    fn propagate_chain_id(mut self) -> Self {
        self.config.gateway.chain_id = self.chain_id.clone();
        // Assuming a valid path.
        self.config.storage.db_config.path.push(self.chain_id.0.as_str());
        self
    }
}
