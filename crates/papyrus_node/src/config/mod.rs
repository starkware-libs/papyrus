#[cfg(test)]
mod config_test;

use std::collections::HashMap;
use std::mem::discriminant;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

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
    ArgParseMatch(#[from] clap::parser::MatchesError),
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
    #[error(transparent)]
    ConfigFile(#[from] io::Error),
}

struct ConfigBuilder {
    args: Option<ArgMatches>,
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
        // TODO: Implement Default in StarknetApi.
        let chain_id = ChainId(String::from("SN_MAIN"));

        ConfigBuilder {
            args: None,
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
            sync: Some(SyncConfig { block_propagation_sleep_duration: Duration::from_secs(10) }),
        }
    }
}

impl ConfigBuilder {
    fn build() -> Result<Config, ConfigError> {
        // TODO: add configuration from env variables.
        let builder = Self::default().prepare_command()?.yaml()?.args()?.propagate_chain_id();
        Ok(Config {
            gateway: builder.gateway,
            central: builder.central,
            monitoring_gateway: builder.monitoring_gateway,
            storage: builder.storage,
            sync: builder.sync,
        })
    }

    // Builds the applications command-line interface.
    fn prepare_command(mut self) -> Result<Self, ConfigError> {
        self.args = Some(
            Command::new("Papyrus")
                .arg(Arg::new("config_file").help("Path to a config file"))
                .arg(Arg::new("chain_id").help("Set the network chain ID"))
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
                .try_get_matches()?,
        );
        Ok(self)
    }

    // Parses a yaml config file and updates the relevant configurations.
    // Absence of a section or a parameter means keeping the current value of the configuration.
    fn yaml(mut self) -> Result<Self, ConfigError> {
        let config = get_yaml_content(&self)?;

        if let Yaml::String(chain_id_str) = &config["chain_id"] {
            self.chain_id = ChainId(chain_id_str.clone());
        }

        if let Some(central_yaml) = parse_section(&config, "central")? {
            self.parse_central_yaml(central_yaml)?;
        }

        if let Some(gateway_yaml) = parse_section(&config, "gateway")? {
            self.parse_gateway_yaml(gateway_yaml)?;
        }

        if let Some(monitoring_gateway_yaml) = parse_section(&config, "monitoring_gateway")? {
            self.parse_monitoring_gateway_yaml(monitoring_gateway_yaml)?;
        }

        if let Some(storage_yaml) = parse_section(&config, "storage_yaml")? {
            self.parse_storage_yaml(storage_yaml)?;
        }

        if let Some(sync_yaml) = parse_section(&config, "sync_yaml")? {
            self.parse_sync_yaml(sync_yaml)?;
        }

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

        fn get_yaml_content(instance: &ConfigBuilder) -> Result<Yaml, ConfigError> {
            let config_file = match instance.args {
                None => unreachable!(),
                Some(ref args) => match args.try_get_one::<String>("config_file") {
                    Err(err) => return Err(ConfigError::ArgParseMatch(err)),
                    Ok(None) => String::from(CONFIG_FILE),
                    Ok(Some(config_file)) => config_file.clone(),
                },
            };

            let config_contents = fs::read_to_string(config_file)?;
            Ok(YamlLoader::load_from_str(config_contents.as_str())?.remove(0))
        }
    }

    // Parse the command-line args and update the relevant config builders.
    fn args(mut self) -> Result<Self, ConfigError> {
        match self.args {
            None => unreachable!(),
            Some(ref args) => {
                if let Some(chain_id) = args.try_get_one::<String>("chain_id")? {
                    self.chain_id = ChainId(chain_id.clone());
                }

                if let Some(storage_path) = args.try_get_one::<String>("storage_path")? {
                    self.storage.db_config.path = storage_path.clone();
                }

                if let Some(no_sync) = args.try_get_one::<bool>("no_sync")? {
                    if *no_sync {
                        self.sync = None;
                    }
                }

                Ok(self)
            }
        }
    }

    // Propagates the chain id into all the of configurations that use it.
    fn propagate_chain_id(mut self) -> Self {
        self.gateway.chain_id = self.chain_id.clone();
        // Assuming a valid path.
        self.storage.db_config.path.push_str(format!("/{}", self.chain_id.0).as_str());
        self
    }

    fn parse_central_yaml(&mut self, central_yaml: &Hash) -> Result<(), ConfigError> {
        let mut config_as_hash = Hash::new();
        let url = Yaml::String("url".to_owned());
        let retry = Yaml::String("retry".to_owned());

        config_as_hash.insert(url.clone(), Yaml::String(self.central.url.clone()));
        config_as_hash.insert(retry.clone(), retry_to_yaml(&self.central.retry_config)?);

        parse_yaml("central", &mut config_as_hash, central_yaml)?;

        self.central.url = config_as_hash.get(&url).unwrap().as_str().unwrap().to_owned();
        self.central.retry_config = yaml_to_retry(config_as_hash.get(&retry).unwrap())?;

        Ok(())
    }

    fn parse_gateway_yaml(&mut self, gateway_yaml: &Hash) -> Result<(), ConfigError> {
        let mut config_as_hash = Hash::new();
        let server_ip = Yaml::String("server_ip".to_owned());
        let max_events_chunk_size = Yaml::String("max_events_chunk_size".to_owned());
        let max_events_keys = Yaml::String("max_events_keys".to_owned());

        config_as_hash.insert(server_ip.clone(), Yaml::String(self.gateway.server_ip.clone()));
        config_as_hash.insert(
            max_events_chunk_size.clone(),
            integer_param_to_yaml(
                self.gateway.max_events_chunk_size,
                "gateway",
                "max_events_chunk_size",
            )?,
        );
        config_as_hash.insert(
            max_events_keys.clone(),
            integer_param_to_yaml(self.gateway.max_events_keys, "gateway", "max_events_keys")?,
        );

        parse_yaml("gateway", &mut config_as_hash, gateway_yaml)?;

        self.gateway.server_ip =
            config_as_hash.get(&server_ip).unwrap().as_str().unwrap().to_owned();
        self.gateway.max_events_chunk_size = yaml_param_to_integer(
            config_as_hash.get(&max_events_chunk_size).unwrap(),
            "gateway",
            "max_events_chunk_size",
        )?;
        self.gateway.max_events_keys = yaml_param_to_integer(
            config_as_hash.get(&max_events_keys).unwrap(),
            "gateway",
            "max_events_keys",
        )?;

        Ok(())
    }

    fn parse_monitoring_gateway_yaml(
        &mut self,
        monitoring_gateway_yaml: &Hash,
    ) -> Result<(), ConfigError> {
        let mut config_as_hash = Hash::new();
        let server_ip = Yaml::String("server_ip".to_owned());

        config_as_hash
            .insert(server_ip.clone(), Yaml::String(self.monitoring_gateway.server_ip.clone()));

        parse_yaml("monitoring_gateway", &mut config_as_hash, monitoring_gateway_yaml)?;

        self.monitoring_gateway.server_ip =
            config_as_hash.get(&server_ip).unwrap().as_str().unwrap().to_owned();

        Ok(())
    }

    fn parse_storage_yaml(&mut self, storage_yaml: &Hash) -> Result<(), ConfigError> {
        let mut config_as_hash = Hash::new();
        let db = Yaml::String("db".to_owned());

        config_as_hash.insert(db.clone(), db_config_to_yaml(&self.storage.db_config)?);

        parse_yaml("storage", &mut config_as_hash, storage_yaml)?;

        self.storage.db_config = yaml_to_db_config(config_as_hash.get(&db).unwrap())?;

        Ok(())
    }

    fn parse_sync_yaml(&mut self, sync_yaml: &Hash) -> Result<(), ConfigError> {
        if self.sync.is_none() {
            return Ok(());
        }
        let mut sync_config = self.sync.as_mut().unwrap();

        let mut config_as_hash = Hash::new();
        let block_propagation_sleep_duration =
            Yaml::String("block_propagation_sleep_duration".to_owned());

        config_as_hash.insert(
            block_propagation_sleep_duration.clone(),
            duration_to_yaml(&sync_config.block_propagation_sleep_duration)?,
        );

        parse_yaml("sync", &mut config_as_hash, sync_yaml)?;

        sync_config.block_propagation_sleep_duration =
            yaml_to_duration(config_as_hash.get(&block_propagation_sleep_duration).unwrap())?;

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
        match param {
            Yaml::Hash(param_as_hash) => {
                // Already checked that v and param have same type so it's ok to unwrap.
                let v_as_hash = v.as_hash().unwrap();
                // Copy from the input only the overriden keys, verify that the key is valid by
                // checking that insert doesn't return None.
                for (k, v) in v_as_hash {
                    param_as_hash.insert(k.clone(), v.clone()).ok_or(ConfigError::YamlKey {
                        section: section.to_owned(),
                        key: k.clone(),
                    })?;
                }
            }
            _ => *param = v.clone(),
        }
    }
    Ok(())
}

fn integer_param_to_yaml<T>(v: T, section: &str, key: &str) -> Result<Yaml, ConfigError>
where
    i64: std::convert::TryFrom<T>,
    <i64 as std::convert::TryFrom<T>>::Error: std::fmt::Display,
{
    Ok(Yaml::Integer(i64::try_from(v).map_err(|e| ConfigError::ConfigValue {
        section: section.to_owned(),
        key: key.to_owned(),
        error: e.to_string(),
    })?))
}

fn yaml_param_to_integer<T>(v: &Yaml, section: &str, key: &str) -> Result<T, ConfigError>
where
    T: std::convert::TryFrom<i64>,
{
    T::try_from(v.as_i64().unwrap()).map_err(|_| ConfigError::YamlParam {
        section: section.to_owned(),
        key: key.to_owned(),
        param: v.clone(),
    })
}

fn retry_to_yaml(retry_config: &RetryConfig) -> Result<Yaml, ConfigError> {
    let mut retry_as_hash = Hash::with_capacity(3);
    retry_as_hash.insert(
        Yaml::String(String::from("retry_base_millis")),
        integer_param_to_yaml(
            retry_config.retry_base_millis,
            "central/retry_config",
            "retry_base_millis",
        )?,
    );
    retry_as_hash.insert(
        Yaml::String(String::from("retry_max_delay_millis")),
        integer_param_to_yaml(
            retry_config.retry_max_delay_millis,
            "central/retry_config",
            "retry_max_delay_millis",
        )?,
    );
    retry_as_hash.insert(
        Yaml::String(String::from("max_retries")),
        integer_param_to_yaml(retry_config.max_retries, "central/retry_config", "max_retries")?,
    );
    Ok(Yaml::Hash(retry_as_hash))
}

fn yaml_to_retry(retry: &Yaml) -> Result<RetryConfig, ConfigError> {
    let retry_as_hash = retry.as_hash().unwrap();
    Ok(RetryConfig {
        retry_base_millis: yaml_param_to_integer(
            retry_as_hash.get(&Yaml::from_str("retry_base_millis")).unwrap(),
            "central/retry",
            "retry_base_millis",
        )?,
        retry_max_delay_millis: yaml_param_to_integer(
            retry_as_hash.get(&Yaml::from_str("retry_max_delay_millis")).unwrap(),
            "central/retry",
            "retry_max_delay_millis",
        )?,
        max_retries: yaml_param_to_integer(
            retry_as_hash.get(&Yaml::from_str("max_retries")).unwrap(),
            "central/retry",
            "max_retries",
        )?,
    })
}

fn db_config_to_yaml(db_config: &DbConfig) -> Result<Yaml, ConfigError> {
    let mut db_config_as_hash = Hash::with_capacity(2);
    db_config_as_hash
        .insert(Yaml::String(String::from("path")), Yaml::String(db_config.path.clone()));
    db_config_as_hash.insert(
        Yaml::String(String::from("max_size")),
        integer_param_to_yaml(db_config.max_size, "storage/db", "max_size")?,
    );
    Ok(Yaml::Hash(db_config_as_hash))
}

fn yaml_to_db_config(db_config: &Yaml) -> Result<DbConfig, ConfigError> {
    let db_config_as_hash = db_config.as_hash().unwrap();
    Ok(DbConfig {
        path: db_config_as_hash.get(&Yaml::from_str("path")).unwrap().as_str().unwrap().to_owned(),
        max_size: yaml_param_to_integer(
            db_config_as_hash.get(&Yaml::from_str("max_size")).unwrap(),
            "storage/db",
            "max_size",
        )?,
    })
}

fn duration_to_yaml(duration: &Duration) -> Result<Yaml, ConfigError> {
    let mut duration_as_hash = Hash::with_capacity(2);
    duration_as_hash.insert(
        Yaml::String(String::from("secs")),
        integer_param_to_yaml(duration.as_secs(), "sync/block_propagation_sleep_duration", "secs")?,
    );
    duration_as_hash.insert(
        Yaml::String(String::from("nanos")),
        integer_param_to_yaml(
            duration.subsec_nanos(),
            "sync/block_propagation_sleep_duration",
            "nanos",
        )?,
    );
    Ok(Yaml::Hash(duration_as_hash))
}

fn yaml_to_duration(duration: &Yaml) -> Result<Duration, ConfigError> {
    let duration_as_hash = duration.as_hash().unwrap();
    Ok(Duration::new(
        yaml_param_to_integer(
            duration_as_hash.get(&Yaml::from_str("secs")).unwrap(),
            "sync/block_propagation_sleep_duration",
            "secs",
        )?,
        yaml_param_to_integer(
            duration_as_hash.get(&Yaml::from_str("nanos")).unwrap(),
            "sync/block_propagation_sleep_duration",
            "nanos",
        )?,
    ))
}
