#[cfg(test)]
mod config_test;

use std::collections::HashMap;
use std::fs;
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::bail;
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

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub gateway: GatewayConfig,
    pub central: CentralSourceConfig,
    pub monitoring_gateway: MonitoringGatewayConfig,
    pub storage: StorageConfig,
    pub sync: Option<SyncConfig>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        ConfigBuilder::default().yaml()?.args()?.build()
    }
}

#[derive(thiserror::Error, Clone, Debug)]
pub enum ConfigError {
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
    gateway: GatewayConfigBuilder,
    // TODO: Add more builders.
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        let chain_id = ChainId(String::from(DEFAULT_CHAIN_ID));
        Self { gateway: GatewayConfigBuilder::new(chain_id) }
    }
}

impl ConfigBuilder {
    fn chain_id(&mut self, chain_id: ChainId) {
        self.gateway.chain_id = chain_id;
    }
    fn yaml(mut self) -> anyhow::Result<Self> {
        let config_contents = fs::read_to_string("config/config.yaml")
            .expect("Something went wrong reading the file");
        let config = YamlLoader::load_from_str(config_contents.as_str())?.remove(0);

        // Notice: BadValue is returned both when the key does'nt exist and when the value type is
        // not valid, so there is no way to check wether the case of param in the config but
        // has an invalid type.
        if let Yaml::String(chain_id_str) = &config["chain_id"] {
            let chain_id = ChainId(chain_id_str.clone());
            self.chain_id(chain_id);
        }

        let gateway_yaml = &config["gateway"];
        match gateway_yaml {
            Yaml::BadValue => {}
            Yaml::Hash(gateway_config) => {
                self.gateway.yaml(gateway_config)?;
            }
            _ => {
                bail!(ConfigError::YamlSection { section: String::from("gateway") });
            }
        }

        Ok(self)
    }

    fn args(mut self) -> anyhow::Result<Self> {
        let args = Command::new("Papyrus")
            .arg(Arg::new("chain_id").value_parser(chain_id_from_str))
            .arg(Arg::new("storage_path").value_parser(value_parser!(PathBuf)))
            .arg(Arg::new("no_sync").default_missing_value("true"))
            .try_get_matches()?;

        if let Some(chain_id) = args.get_one::<ChainId>("chain_id") {
            self.chain_id(chain_id.clone());
        }

        // TODO: Handle other flags

        return Ok(self);

        fn chain_id_from_str(s: &str) -> anyhow::Result<ChainId> {
            Ok(ChainId(s.to_owned()))
        }
    }

    fn build(self) -> anyhow::Result<Config> {
        Ok(Config {
            gateway: self.gateway.build(),
            // TODO: Create builders for the rest of the config structs.
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
            sync: Some(SyncConfig { block_propagation_sleep_duration: Duration::from_secs(10) }),
        })
    }
}

struct GatewayConfigBuilder {
    chain_id: ChainId,
    server_ip: String,
    max_events_chunk_size: usize,
    max_events_keys: usize,
}

impl GatewayConfigBuilder {
    fn new(chain_id: ChainId) -> Self {
        Self {
            chain_id,
            server_ip: String::from("localhost:8080"),
            max_events_chunk_size: 1000,
            max_events_keys: 100,
        }
    }

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

    fn build(self) -> GatewayConfig {
        GatewayConfig {
            chain_id: self.chain_id,
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
    usize::try_from(v.as_i64().unwrap()).map_err(|e| ConfigError::ConfigValue {
        section: section.to_owned(),
        key: key.to_owned(),
        error: e.to_string(),
    })
}
