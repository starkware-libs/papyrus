#[cfg(test)]
mod config_test;

use std::collections::HashMap;
use std::fs;
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;

use clap::{value_parser, Arg, ArgMatches, Command};
use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::{DbConfig, StorageConfig};
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;
use yaml_rust::yaml::Hash;
use yaml_rust::{Yaml, YamlLoader};

const DEFAULT_CHAIN_ID: &str = "SN_GOERLI";

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
    /// Builds the configuration for the various components of the node.
    /// Order of precedence when configuring Papyrus node:
    ///     - CLI
    ///     - Configuration file (path: config/config.yaml)
    ///     - Default values
    pub fn load() -> Result<Self, ConfigError> {
        // TODO: add configuration from env variables.
        Ok(ConfigBuilder::default().yaml()?.args()?.build())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error(transparent)]
    CLI(#[from] clap::Error),
    #[error(transparent)]
    YamlScan(#[from] yaml_rust::ScanError),
    #[error("Invalid config section: {section}")]
    YamlSection { section: String },
    #[error("Invalid config key '{key:?}' in {section}")]
    YamlKey { section: String, key: Yaml },
    #[error("Invalid config parameter '{param:?}' in {section}::{key}")]
    YamlParam { section: String, key: String, param: Yaml },
    #[error("Bad value in {section}::{key}. {error}")]
    ConfigValue { section: String, key: String, error: String },
}

// Keeps the configuration parameters in order to build an instance of Config.
// Uses the Builder pattern: https://doc.rust-lang.org/1.0.0/style/ownership/builders.html
// Every step updates the configuration values from which the final members of the Config instance
// will be built.
struct ConfigBuilder {
    chain_id: ChainId,
    // TODO: do we really need a builder for each component, or simply modifing the original struct
    // is enough?
    gateway: GatewayConfigBuilder,
    // TODO: Add more components.
}

// Default configuration values.
impl Default for ConfigBuilder {
    fn default() -> Self {
        ConfigBuilder {
            chain_id: ChainId(String::from(DEFAULT_CHAIN_ID)),
            gateway: GatewayConfigBuilder::default(),
        }
    }
}

impl ConfigBuilder {
    // Parses a yaml config file and updates the relevant config builders.
    // Absence of a section or a parameter means keeping the current value of the config builder.
    fn yaml(mut self) -> Result<Self, ConfigError> {
        let config_contents = fs::read_to_string("config/config.yaml")
            .expect("Something went wrong reading the file");
        let config = YamlLoader::load_from_str(config_contents.as_str())?.remove(0);

        // Notice: BadValue is returned both when the key doesn't exist and when the value type is
        // not valid, so there is no way to check whether chain_id is in the config but has an
        // invalid type.
        if let Yaml::String(chain_id_str) = &config["chain_id"] {
            self.chain_id = ChainId(chain_id_str.clone());
        }

        let gateway_yaml = &config["gateway"];
        match gateway_yaml {
            Yaml::BadValue => {} // Received when there is no gateway section in the config file.
            Yaml::Hash(gateway_config) => {
                self.gateway.yaml(gateway_config)?;
            }
            _ => {
                return Err(ConfigError::YamlSection { section: String::from("gateway") });
            }
        }

        Ok(self)
    }

    /// Parse the CLI and update the relevant config builders.
    fn args(mut self) -> Result<Self, ConfigError> {
        let args = Command::new("Papyrus")
            .arg(
                Arg::new("chain_id")
                    .help("Set the network chain ID")
                    .value_parser(chain_id_from_str),
            )
            .arg(
                Arg::new("storage_path")
                    .help("Set the path of the node's storage")
                    .value_parser(value_parser!(PathBuf)),
            )
            .arg(
                Arg::new("no_sync")
                    .help("Run the node without syncing")
                    .default_missing_value("true"),
            )
            .try_get_matches()?;

        if let Some(chain_id) = args.get_one::<ChainId>("chain_id") {
            self.chain_id = chain_id.clone();
        }

        // TODO: Handle other flags

        return Ok(self);

        fn chain_id_from_str(s: &str) -> Result<ChainId, ConfigError> {
            // TODO: add checks on the chain_id (not empty, no spaces, etc.).
            Ok(ChainId(s.to_owned()))
        }
    }

    // Builds each components configuration based on the stored values
    fn build(self) -> Config {
        Config {
            // TODO: Do we really need 'build' method here, or a simple constructor is enough?
            gateway: self.gateway.build(&self.chain_id),
            // TODO: delete these instances and create builders for the rest of the config structs
            // based on the stored values (like in the gateway).
            central: CentralSourceConfig {
                url: String::from("https://alpha4.starknet.io/"),
                retry_config: RetryConfig {
                    retry_base_millis: 30,
                    retry_max_delay_millis: 30000,
                    max_retries: 10,
                },
            },
            monitoring_gateway: MonitoringGatewayConfig {
                server_ip: String::from("localhost::8081"),
            },
            storage: StorageConfig {
                db_config: DbConfig { path: String::from("./data"), max_size: 1099511627776 },
            },
            // None value means no syncing.
            // TODO: set None if no_sync flag was passed.
            sync: Some(SyncConfig { block_propagation_sleep_duration: Duration::from_secs(10) }),
        }
    }
}

struct GatewayConfigBuilder {
    server_ip: String,
    max_events_chunk_size: usize,
    max_events_keys: usize,
}

impl Default for GatewayConfigBuilder {
    fn default() -> Self {
        Self {
            server_ip: String::from("localhost:8080"),
            max_events_chunk_size: 1000,
            max_events_keys: 100,
        }
    }
}

impl GatewayConfigBuilder {
    fn yaml(&mut self, gateway_yaml: &Hash) -> Result<(), ConfigError> {
        let mut config = Hash::new();
        let server_ip = Yaml::String("server_ip".to_owned());
        let max_events_chunk_size = Yaml::String("max_events_chunk_size".to_owned());
        let max_events_keys = Yaml::String("max_events_keys".to_owned());

        config.insert(server_ip.clone(), Yaml::String(self.server_ip.clone()));
        config.insert(
            max_events_chunk_size.clone(),
            usize_param_to_yaml(self.max_events_chunk_size, "gateway", "max_events_chunk_size")?,
        );
        config.insert(
            max_events_keys.clone(),
            usize_param_to_yaml(self.max_events_keys, "gateway", "max_events_keys")?,
        );

        parse_yaml("gateway", &mut config, gateway_yaml)?;

        self.server_ip = config.get(&server_ip).unwrap().as_str().unwrap().to_owned();
        self.max_events_chunk_size = yaml_param_to_usize(
            config.get(&max_events_chunk_size).unwrap(),
            "gateway",
            "max_events_chunk_size",
        )?;
        self.max_events_keys = yaml_param_to_usize(
            config.get(&max_events_keys).unwrap(),
            "gateway",
            "max_events_keys",
        )?;

        Ok(())
    }

    fn build(self, chain_id: &ChainId) -> GatewayConfig {
        GatewayConfig {
            chain_id: chain_id.clone(),
            server_ip: self.server_ip,
            max_events_chunk_size: self.max_events_chunk_size,
            max_events_keys: self.max_events_keys,
        }
    }
}

fn parse_yaml(section: &str, configuration: &mut Hash, input: &Hash) -> Result<(), ConfigError> {
    for (k, v) in input {
        let param = configuration
            .get_mut(k)
            .ok_or(ConfigError::YamlKey { section: section.to_owned(), key: k.clone() })?;

        // Check that the variant of the input is as expected.
        if discriminant(param) != discriminant(v) {
            let key = k.as_str().expect("Error while parsing configuration").to_owned();
            return Err(ConfigError::YamlParam {
                section: section.to_owned(),
                key,
                param: v.clone(),
            });
        }
        *param = v.clone();
    }
    Ok(())
}

fn usize_param_to_yaml(v: usize, section: &str, key: &str) -> Result<Yaml, ConfigError> {
    Ok(Yaml::Integer(i64::try_from(v).map_err(|e| ConfigError::ConfigValue {
        section: section.to_owned(),
        key: key.to_owned(),
        error: e.to_string(),
    })?))
}

fn yaml_param_to_usize(v: &Yaml, section: &str, key: &str) -> Result<usize, ConfigError> {
    usize::try_from(v.as_i64().unwrap()).map_err(|_| ConfigError::YamlParam {
        section: section.to_owned(),
        key: key.to_owned(),
        param: v.clone(),
    })
}
