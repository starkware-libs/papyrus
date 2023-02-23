use std::collections::HashMap;
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

use crate::config::file_config::FileConfigFormat;
use clap::{arg, value_parser, Arg, ArgMatches, Command};
use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::db::DbConfig;
use papyrus_storage::StorageConfig;
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;

use super::provider::ConfigProvider;
use super::{Config, ConfigError};

// Builds the configuration for the node based on default values, yaml configuration file and
// command-line arguments.
// TODO: add configuration from env variables.
#[derive(Clone)]
pub(crate) struct ConfigBuilder {
    pub(crate) args: Option<ArgMatches>,
    pub(crate) chain_id: ChainId,
    pub(crate) config: Config,
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
    pub(crate) fn build<C>(args: Vec<String>, providers: Vec<Box<C>>) -> Result<Config, ConfigError>
    where
        C: ConfigProvider + ?Sized,
    {
        let config: Result<ConfigBuilder, ConfigError> = providers.iter().fold(
            Ok(Self::default().prepare_command(args).unwrap()),
            |config, p: &Box<C>| -> Result<ConfigBuilder, ConfigError> {
                let builder = p.apply_config(&mut config?)?;
                Ok(builder)
            },
        );

        Ok(config?.config)
    }

    // Builds the applications command-line interface.
    pub(crate) fn prepare_command(mut self, args: Vec<String>) -> Result<Self, ConfigError> {
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
            ])
            .try_get_matches_from(args).unwrap_or_else(|e| e.exit()),
        );
        Ok(self)
    }

    // Reads the command-line args and updates the relevant configurations.
    pub(crate) fn args(mut self) -> Result<Self, ConfigError> {
        match self.args {
            None => unreachable!(),
            Some(ref args) => Ok(self),
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
