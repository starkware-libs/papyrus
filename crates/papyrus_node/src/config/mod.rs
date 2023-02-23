#[cfg(test)]
mod config_test;

mod builder;
mod file_config;
mod provider;

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

use self::builder::ConfigBuilder;
use self::provider::{
    ClaConfigProvider, ConfigProvider, EnvConfigProvider, YamlFileConfigProvider,
};

// The path of the default configuration file, provided as part of the crate.

/// The configurations of the various components of the node.
#[derive(Debug, Deserialize, Serialize, Clone)]
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
        let providers: Vec<Box<dyn ConfigProvider>> = vec![
            Box::new(YamlFileConfigProvider::new()),
            Box::new(EnvConfigProvider::new(vec![
                "PAPYRUS_STORAGE_DB_PATH",
                "PAPYRUS_GATEWAY_SERVER_ADDRESS",
            ])),
            Box::new(ClaConfigProvider::new()),
        ];
        ConfigBuilder::build(args, providers)
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
    #[error("Arguments not prepared")]
    ArgsNotPrepared,
}
