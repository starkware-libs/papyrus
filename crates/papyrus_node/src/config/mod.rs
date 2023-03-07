#[cfg(test)]
mod config_test;

mod file_config;

use std::collections::HashMap;
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

use clap::{arg, value_parser, Arg, ArgMatches, Command};
use file_config::FileConfigFormat;
use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;

// The path of the default configuration file, provided as part of the crate.
const CONFIG_FILE: &str = "config/config.yaml";

/// The configurations of the various components of the node.
#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub gateway: GatewayConfig,
    pub central: CentralSourceConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    /// None if the syncing should be disabled.
    pub sync: Option<SyncConfig>,
}

impl Config {
    pub fn load(args: Vec<String>) -> Result<Self, ConfigError> {
        ConfigBuilder::build(args)
    }

    pub fn get_config_representation(&self) -> Result<serde_yaml::Value, ConfigError> {
        Ok(serde_yaml::to_value(FileConfigFormat::from(self.clone()))?)
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
// TODO: Consider implementing Default for each component individually.
impl Default for ConfigBuilder {
    fn default() -> Self {
        let chain_id = ChainId(String::from("SN_MAIN"));

        ConfigBuilder {
            args: None,
            chain_id: chain_id.clone(),
            config: Config {
                central: CentralSourceConfig {
                    concurrent_requests: 300,
                    url: String::from("https://alpha-mainnet.starknet.io/"),
                    http_headers: None,
                    retry_config: RetryConfig {
                        retry_base_millis: 30,
                        retry_max_delay_millis: 30000,
                        max_retries: 10,
                    },
                },
                gateway: GatewayConfig {
                    chain_id,
                    server_address: String::from("0.0.0.0:8080"),
                    max_events_chunk_size: 1000,
                    max_events_keys: 100,
                },
                monitoring_gateway: MonitoringGatewayConfig {
                    server_address: String::from("0.0.0.0:8081"),
                },
                storage: StorageConfig {
                    db_config: DbConfig { path: String::from("./data"), max_size: 1099511627776 },
                },
                sync: Some(SyncConfig {
                    block_propagation_sleep_duration: Duration::from_secs(10),
                    recoverable_error_sleep_duration: Duration::from_secs(10),
                }),
            },
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
            Command::new("Papyrus",)
            .version("Pre-release")
            .about("Papyrus is a StarkNet full node written in Rust.")
            .args(&[
                arg!(-f --config_file [path] "Optionally sets a config file to use").value_parser(value_parser!(PathBuf)),
                arg!(-c --chain_id [name] "Optionally sets chain id to use"),
                arg!(--server_address ["IP:PORT"] "Optionally sets the RPC listening address"),
                arg!(--http_headers ["NAME:VALUE"] ... "Optionally adds headers to the http requests"),
                arg!(-s --storage [path] "Optionally sets storage path to use (automatically extended with chain ID)").value_parser(value_parser!(PathBuf)),
                arg!(-n --no_sync [bool] "Optionally run without sync").value_parser(value_parser!(bool)).default_missing_value("true"),
                arg!(--central_url ["URL"] "Central URL. It should match chain_id."),
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
                    self.config.storage.db_config.path = storage_path
                        .to_str()
                        .ok_or(ConfigError::BadPath { path: storage_path.clone() })?
                        .to_owned();
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

                Ok(self)
            }
        }
    }

    // Propagates the chain id into all the of configurations that use it.
    fn propagate_chain_id(mut self) -> Self {
        self.config.gateway.chain_id = self.chain_id.clone();
        // Assuming a valid path.
        self.config.storage.db_config.path.push_str(format!("/{}", self.chain_id.0).as_str());
        self
    }
}
