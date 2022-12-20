#[cfg(test)]
mod config_test;

mod file_config;

use std::collections::HashMap;
use std::env::{args, ArgsOs};
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

use clap::{arg, value_parser, Arg, ArgMatches, Command};
use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::{DbConfig, StorageConfig};
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;
use yaml_rust::yaml::Hash;
use yaml_rust::{Yaml, YamlLoader};

use crate::config::file_config::apply_yaml_config;

// The path of the default configuration file, provided as part of the crate.
const CONFIG_FILE: &str = "config/config.yaml";

/// The configurations of the various components of the node.
#[derive(Deserialize, Serialize)]
pub struct Config {
    pub gateway: GatewayConfig,
    pub central: CentralSourceConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    /// None if the syncing should be disabled.
    pub sync: Option<SyncConfig>,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        ConfigBuilder::build()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error(transparent)]
    Clap(#[from] clap::Error),
    #[error(transparent)]
    Matches(#[from] clap::parser::MatchesError),
    #[error(transparent)]
    Read(#[from] io::Error),
    #[error(transparent)]
    Serde(#[from] serde_yaml::Error),
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
        // TODO: Implement Default in StarknetApi.
        let chain_id = ChainId(String::from("SN_MAIN"));

        ConfigBuilder {
            args: None,
            chain_id: chain_id.clone(),
            config: Config {
                central: CentralSourceConfig {
                    url: String::from("https://alpha4.starknet.io/"),
                    retry_config: RetryConfig {
                        retry_base_millis: 30,
                        retry_max_delay_millis: 30000,
                        max_retries: 10,
                    },
                },
                gateway: GatewayConfig {
                    chain_id,
                    server_ip: String::from("localhost:8080"),
                    max_events_chunk_size: 1000,
                    max_events_keys: 100,
                },
                monitoring_gateway: MonitoringGatewayConfig {
                    server_ip: String::from("localhost:8081"),
                },
                storage: StorageConfig {
                    db_config: DbConfig { path: String::from("./data"), max_size: 1099511627776 },
                },
                sync: Some(SyncConfig {
                    block_propagation_sleep_duration: Duration::from_secs(10),
                }),
            },
        }
    }
}

impl ConfigBuilder {
    // Creates the configuration struct.
    fn build() -> Result<Config, ConfigError> {
        Ok(Self::default()
            .prepare_command(args().collect())?
            .yaml()?
            .args()?
            .propagate_chain_id()
            .config)
    }

    // Builds the applications command-line interface.
    fn prepare_command(mut self, args: Vec<String>) -> Result<Self, ConfigError> {
        self.args = Some(
            Command::new("Papyrus").args(&[
                arg!(-f --config [FILE] "Optionally sets a config file to use"),
                arg!(-c --chain_id [CHAIN_ID] "Optionally sets chain id to use"),
                arg!(-s --storage [PATH] "Optionally sets storage path to use (automatically extended with chain id").value_parser(value_parser!(PathBuf)),
                arg!(-n --no_sync [BOOL] "Optionally run without sync").value_parser(value_parser!(bool)).default_missing_value("true"),
            ])
            .try_get_matches_from(args)?,
        );
        Ok(self)
    }

    // Parses a yaml configuration file given by the command-line args (or default), and applies it
    // on the configuration.
    fn yaml(self) -> Result<Self, ConfigError> {
        let config = match self
            .args
            .clone()
            .expect("Config builder should have args.")
            .try_get_one::<String>("config")?
        {
            None => String::from(CONFIG_FILE),
            Some(config_file) => config_file.clone(),
        };

        apply_yaml_config(self, config.as_str())
    }

    // Reads the command-line args and updates the relevant configurations.
    fn args(mut self) -> Result<Self, ConfigError> {
        match self.args {
            None => unreachable!(),
            Some(ref args) => {
                if let Some(chain_id) = args.try_get_one::<String>("chain_id")? {
                    self.chain_id = ChainId(chain_id.clone());
                }

                if let Some(storage_path) = args.try_get_one::<String>("storage")? {
                    self.config.storage.db_config.path = storage_path.clone();
                }

                if let Some(no_sync) = args.try_get_one::<bool>("no_sync")? {
                    if *no_sync {
                        self.config.sync = None;
                    }
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
