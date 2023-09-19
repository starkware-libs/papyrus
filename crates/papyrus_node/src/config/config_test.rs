use std::collections::{BTreeMap, HashMap};
use std::env::{self, args};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::IndexMut;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use itertools::Itertools;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::representation::get_config_representation;
use papyrus_config::{SerializationType, SerializedContent, SerializedParam};
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use pretty_assertions::assert_eq;
use serde_json::{json, Map, Value};
use starknet_api::core::ChainId;
use tempfile::NamedTempFile;
use test_utils::get_absolute_path;
use validator::Validate;

use crate::config::{node_command, NodeConfig, DEFAULT_CONFIG_PATH};

// Returns the required and generated params in default_config.json with the default value from the
// config representation.
fn required_args() -> Vec<String> {
    let default_config = NodeConfig::default();
    let mut args = Vec::new();
    let mut config_representation = get_config_representation(&default_config, true).unwrap();

    for (param_path, serialized_param) in default_config.dump() {
        let serialization_type = match serialized_param.content {
            SerializedContent::DefaultValue(_) | SerializedContent::PointerTarget(_) => continue,
            SerializedContent::RequiredType(serialization_type) => serialization_type,
            SerializedContent::GeneratedType(serialization_type) => serialization_type,
        };
        args.push(format!("--{param_path}"));

        let required_param_json_value = param_path
            .split('.')
            .fold(&mut config_representation, |entry, config_name| entry.index_mut(config_name));

        let required_param_string_value = match serialization_type {
            SerializationType::String => required_param_json_value.as_str().unwrap().to_string(),
            _ => required_param_json_value.to_string(),
        };
        args.push(required_param_string_value);
    }
    args
}

fn get_args(additional_args: Vec<&str>) -> Vec<String> {
    let mut args = vec!["Papyrus".to_owned()];
    args.append(&mut required_args());
    args.append(&mut additional_args.into_iter().map(|s| s.to_owned()).collect());
    args
}

#[test]
fn load_default_config() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    NodeConfig::load_and_process(get_args(vec![])).expect("Failed to load the config.");
}

#[test]
fn load_http_headers() {
    let args = get_args(vec!["--central.http_headers", "NAME_1:VALUE_1 NAME_2:VALUE_2"]);
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let config = NodeConfig::load_and_process(args).unwrap();
    let target_http_headers = HashMap::from([
        ("NAME_1".to_string(), "VALUE_1".to_string()),
        ("NAME_2".to_string(), "VALUE_2".to_string()),
    ]);
    assert_eq!(config.central.http_headers.unwrap(), target_http_headers);
}

#[test]
// Regression test which checks that the default config dumping hasn't changed.
fn test_dump_default_config() {
    let default_config = NodeConfig::default();
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    default_config.validate().unwrap();
    let dumped_default_config = default_config.dump();
    insta::assert_json_snapshot!(dumped_default_config);
}

#[test]
fn test_default_config_process() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    assert_eq!(NodeConfig::load_and_process(get_args(vec![])).unwrap(), NodeConfig::default());
}

#[test]
fn test_update_dumped_config_by_command() {
    let args =
        get_args(vec!["--rpc.max_events_keys", "1234", "--storage.db_config.path_prefix", "/abc"]);
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let config = NodeConfig::load_and_process(args).unwrap();

    assert_eq!(config.rpc.max_events_keys, 1234);
    assert_eq!(config.storage.db_config.path_prefix.to_str(), Some("/abc"));
}
