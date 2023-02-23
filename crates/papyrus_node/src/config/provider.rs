use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use serde_yaml::{Mapping, Value};
use starknet_api::core::ChainId;
use tracing::debug;

use super::{builder::ConfigBuilder, ConfigError};

const CONFIG_FILE: &str = "config/config.yaml";

pub(crate) trait ConfigProvider {
    fn apply_config(&self, config: &mut ConfigBuilder) -> Result<ConfigBuilder, ConfigError>;
}

pub(crate) struct YamlFileConfigProvider<'a> {
    file_path: &'a str,
}

fn compose_parent_key<'a>(parent_key: &'a str, key: &'a str) -> String {
    if "" == parent_key {
        return format!("{key}");
    }

    format!("{parent_key}.{key}")
}

impl YamlFileConfigProvider<'_> {
    pub(crate) fn new() -> Self {
        Self { file_path: CONFIG_FILE }
    }

    fn parse_yaml_file(&self, config: &ConfigBuilder) -> Result<Mapping, ConfigError> {
        let mut yaml_path = self.file_path;

        let args = config.args.clone().expect("Config builder should have args.");
        if let Some(config_file) = args.try_get_one::<PathBuf>("config_file")? {
            yaml_path =
                config_file.to_str().ok_or(ConfigError::BadPath { path: config_file.clone() })?;
        }

        let yaml_contents = fs::read_to_string(yaml_path)?;
        let value: serde_yaml::Value = serde_yaml::from_str(&yaml_contents.to_string())?;

        Ok(value.as_mapping().unwrap().to_owned())
    }

    fn apply_config_item<'a, 'b>(
        &'a self,
        config: &'b mut ConfigBuilder,
        mapping: Value,
        parent_key: &'b str,
    ) -> &'b mut ConfigBuilder {
        for (key, value) in mapping.as_mapping().unwrap() {
            if value.is_mapping() {
                self.apply_config_item(
                    config,
                    value.to_owned(),
                    compose_parent_key(parent_key, key.as_str().unwrap_or("")).as_str(),
                );
            }

            match key.as_str().unwrap() {
                "chain_id" => {
                    config.config.gateway.chain_id = ChainId(value.as_str().unwrap().to_string());
                }
                "concurrent_requests" => {
                    config.config.central.concurrent_requests =
                        usize::try_from(value.as_u64().unwrap())
                            .expect("Failed to parse out value of central.concurrent_requests");
                }
                "url" => config.config.central.url = value.as_str().unwrap().to_string(),
                "retry_base_millis" => {
                    config.config.central.retry_config.retry_base_millis = value.as_u64().unwrap()
                }
                "retry_max_delay_millis" => {
                    config.config.central.retry_config.retry_max_delay_millis =
                        value.as_u64().unwrap()
                }
                "max_retries" => {
                    config.config.central.retry_config.max_retries =
                        usize::try_from(value.as_u64().unwrap())
                            .expect("Failed to parse out value of central.retry.max_retries");
                }
                "http_headers" => {
                    let yaml_config_headers = value.as_mapping().unwrap().into_iter().fold(
                        HashMap::new(),
                        |mut acc, curr| {
                            acc.insert(
                                curr.0.as_str().unwrap().to_string(),
                                curr.1.as_str().unwrap().to_string(),
                            );
                            acc
                        },
                    );
                    match &mut config.config.central.http_headers {
                        Some(headers) => headers.extend(yaml_config_headers),
                        None => {
                            config.config.central.http_headers = Some(yaml_config_headers);
                        }
                    };
                }
                "server_address" => match parent_key {
                    "gateway" => {
                        config.config.gateway.server_address = value.as_str().unwrap().to_string()
                    }
                    "monitoring_gateway" => {
                        config.config.monitoring_gateway.server_address =
                            value.as_str().unwrap().to_string()
                    }
                    _ => debug!(
                        "yaml configuration with key {} not handled",
                        compose_parent_key(parent_key, key.as_str().unwrap())
                    ),
                },
                "max_events_chunk_size" => {
                    config.config.gateway.max_events_chunk_size =
                        usize::try_from(value.as_u64().unwrap())
                            .expect("Failed to parse out value of gateway.max_events_chunk_size");
                }
                "max_events_keys" => {
                    config.config.gateway.max_events_keys =
                        usize::try_from(value.as_u64().unwrap())
                            .expect("Failed to parse out value of gateway.max_events_keys");
                }
                "path" => {
                    config.config.storage.db_config.path = value.as_str().unwrap().to_string();
                }
                "max_size" => {
                    config.config.storage.db_config.max_size =
                        usize::try_from(value.as_u64().unwrap())
                            .expect("Failed to parse out value of storage.db.max_size");
                }
                _ => debug!(
                    "yaml configuration with key {} not handled",
                    compose_parent_key(parent_key, key.as_str().unwrap())
                ),
            }
        }

        config
    }
}
impl ConfigProvider for YamlFileConfigProvider<'_> {
    fn apply_config(&self, config: &mut ConfigBuilder) -> Result<ConfigBuilder, ConfigError> {
        let parsed_value = self.parse_yaml_file(config)?;
        self.apply_config_item(config, serde_yaml::Value::Mapping(parsed_value), "");
        Ok(config.clone())
    }
}

#[derive(Default)]
pub(crate) struct EnvConfigProvider<'a> {
    variables_keys: Vec<&'a str>,
}

impl<'a> EnvConfigProvider<'a> {
    pub(crate) fn new(keys: Vec<&'a str>) -> Self {
        Self { variables_keys: keys }
    }

    fn parse_config_source(
        &self,
        config: &ConfigBuilder,
    ) -> Result<HashMap<String, String>, ConfigError> {
        let env_vars = std::env::vars();
        Ok(env_vars.fold(HashMap::new(), |mut acc: HashMap<String, String>, (key, value)| {
            if !self.variables_keys.contains(&key.as_str()) {
                return acc;
            }
            acc.insert(key, value);
            acc
        }))
    }
}
impl<'a> ConfigProvider for EnvConfigProvider<'a> {
    fn apply_config(&self, config: &mut ConfigBuilder) -> Result<ConfigBuilder, ConfigError> {
        let env_values = self.parse_config_source(config)?;
        for (key, value) in env_values {
            match key.as_str() {
                "PAPYRUS_STORAGE_DB_PATH" => config.config.storage.db_config.path = value,
                "PAPYRUS_GATEWAY_SERVER_ADDRESS" => config.config.gateway.server_address = value,
                _ => debug!("env configuration with key {} not handled", key),
            }
        }
        Ok(config.clone())
    }
}

pub(crate) struct ClaConfigProvider {}

impl ClaConfigProvider {
    pub(crate) fn new() -> Self {
        Self {}
    }
}
impl ConfigProvider for ClaConfigProvider {
    fn apply_config(&self, config: &mut ConfigBuilder) -> Result<ConfigBuilder, ConfigError> {
        let args = config.args.as_mut().expect("config arguments must have been set");

        if let Some(chain_id) = args.try_get_one::<String>("chain_id")? {
            config.config.gateway.chain_id = ChainId(chain_id.clone());
        }

        if let Some(server_address) = args.try_get_one::<String>("server_address")? {
            config.config.gateway.server_address = server_address.to_string()
        }

        if let Some(storage_path) = args.try_get_one::<PathBuf>("storage")? {
            config.config.storage.db_config.path = storage_path
                .to_str()
                .ok_or(ConfigError::BadPath { path: storage_path.clone() })?
                .to_owned();
        }

        if let Some(http_headers) = args.try_get_one::<String>("http_headers")? {
            let mut headers_map = match &config.config.central.http_headers {
                Some(map) => map.clone(),
                None => HashMap::new(),
            };
            for header in http_headers.split(' ') {
                let split: Vec<&str> = header.split(':').collect();
                if split.len() != 2 {
                    return Err(ConfigError::CLAHttpHeader { illegal_header: header.to_string() });
                }
                headers_map.insert(split[0].to_string(), split[1].to_string());
            }
            config.config.central.http_headers = Some(headers_map);
        }

        if let Some(no_sync) = args.try_get_one::<bool>("no_sync")? {
            if *no_sync {
                config.config.sync = None;
            }
        }

        Ok(config.clone())
    }
}
