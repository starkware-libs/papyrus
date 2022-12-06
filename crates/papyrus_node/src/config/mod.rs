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

struct ConfigBuilder {
    chain_id: ChainId,
    gateway: GatewayConfig,
    central: CentralSourceConfig,
    monitoring_gateway: MonitoringGatewayConfig,
    storage: StorageConfig,
    sync: Option<SyncConfig>,
}

// Default configuration values.
impl Default for ConfigBuilder {
    fn default() -> Self {
        let chain_id = ChainId(String::from(DEFAULT_CHAIN_ID));

        ConfigBuilder {
            chain_id: chain_id.clone(),
            central: CentralSourceConfig {
                url: String::from("https://alpha4.starknet.io/"),
                retry_config: RetryConfig {
                    retry_base_millis: 30,
                    retry_max_delay_millis: 30000,
                    max_retries: 10,
                },
            },
            gateway: GatewayConfig {
                chain_id: chain_id.clone(),
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
            sync: Some(SyncConfig { block_propagation_sleep_duration: Duration::from_secs(10) }),
        }
    }
}

impl ConfigBuilder {
    fn build() -> Result<Config, ConfigError> {
        // TODO: add configuration from env variables.
        let builder = Self::default().yaml()?.args()?.propagate_chain_id();
        Ok(Config {
            gateway: builder.gateway,
            central: builder.central,
            monitoring_gateway: builder.monitoring_gateway,
            storage: builder.storage,
            sync: builder.sync,
        })
    }

    // Parses a yaml config file and updates the relevant configurations.
    // Absence of a section or a parameter means keeping the current value of the configuration.
    fn yaml(mut self) -> Result<Self, ConfigError> {
        let config_contents =
            fs::read_to_string(CONFIG_FILE).expect("Something went wrong reading the file");
        let config = YamlLoader::load_from_str(config_contents.as_str())?.remove(0);

        if let Yaml::String(chain_id_str) = &config["chain_id"] {
            self.chain_id = ChainId(chain_id_str.clone());
        }

        if let Some(gateway_yaml) = parse_section(&config, "gateway")? {
            self.gateway_yaml(gateway_yaml)?;
        }

        // TODO: the rest of the components.

        return Ok(self);

        fn parse_section<'a>(
            yaml: &'a Yaml,
            section: &'a str,
        ) -> Result<Option<&'a Hash>, ConfigError> {
            let section_yaml = &yaml[section];
            match section_yaml {
                Yaml::BadValue => Ok(None), // The component wasn't configured in the yaml.
                Yaml::Hash(hash) => Ok(Some(hash)),
                _ => Err(ConfigError::YamlSection { section: section.to_owned() }),
            }
        }
    }

    // Parse the CLI and update the relevant config builders.
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

        // TODO: set other components.

        return Ok(self);

        fn chain_id_from_str(s: &str) -> Result<ChainId, ConfigError> {
            // TODO: add checks on the chain_id (not empty, no spaces, etc.).
            Ok(ChainId(s.to_owned()))
        }
    }

    // Propagates the chain id to all the of configurations that use it.
    fn propagate_chain_id(mut self) -> Self {
        self.gateway.chain_id = self.chain_id.clone();
        self.storage.db_config.path.push_str(format!("/{}", self.chain_id.0).as_str());
        self
    }

    fn gateway_yaml(&mut self, gateway_yaml: &Hash) -> Result<(), ConfigError> {
        let mut config_as_hash = Hash::new();
        let server_ip = Yaml::String("server_ip".to_owned());
        let max_events_chunk_size = Yaml::String("max_events_chunk_size".to_owned());
        let max_events_keys = Yaml::String("max_events_keys".to_owned());

        config_as_hash.insert(server_ip.clone(), Yaml::String(self.gateway.server_ip.clone()));
        config_as_hash.insert(
            max_events_chunk_size.clone(),
            usize_param_to_yaml(
                self.gateway.max_events_chunk_size,
                "gateway",
                "max_events_chunk_size",
            )?,
        );
        config_as_hash.insert(
            max_events_keys.clone(),
            usize_param_to_yaml(self.gateway.max_events_keys, "gateway", "max_events_keys")?,
        );

        parse_yaml("gateway", &mut config_as_hash, gateway_yaml)?;

        self.gateway.server_ip =
            config_as_hash.get(&server_ip).unwrap().as_str().unwrap().to_owned();
        self.gateway.max_events_chunk_size = yaml_param_to_usize(
            config_as_hash.get(&max_events_chunk_size).unwrap(),
            "gateway",
            "max_events_chunk_size",
        )?;
        self.gateway.max_events_keys = yaml_param_to_usize(
            config_as_hash.get(&max_events_keys).unwrap(),
            "gateway",
            "max_events_keys",
        )?;

        Ok(())
    }
}

// Gets the preconfigured params of a section in a &mut Has, and the parameters of this section from
// the Yaml file, and updates the configuration with the parameters from the file, while running
// checks on the validity of the configuration.
fn parse_yaml(section: &str, configuration: &mut Hash, input: &Hash) -> Result<(), ConfigError> {
    for (k, v) in input {
        let param = configuration
            .get_mut(k)
            // Invalid key in the config file.
            .ok_or(ConfigError::YamlKey { section: section.to_owned(), key: k.clone() })?;

        // Invalid value type of the configuration.
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
