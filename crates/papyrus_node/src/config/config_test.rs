use std::collections::{BTreeMap, HashMap};
use std::env::{self, args};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::ops::IndexMut;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use assert_json_diff::assert_json_eq;
use colored::Colorize;
use itertools::Itertools;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::presentation::get_config_presentation;
use papyrus_config::{SerializationType, SerializedContent, SerializedParam};
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use pretty_assertions::assert_eq;
use serde_json::{json, Map, Value};
use starknet_api::core::ChainId;
use tempfile::NamedTempFile;
use test_utils::get_absolute_path;
use validator::Validate;

use crate::config::{node_command, NodeConfig, CONFIG_POINTERS, DEFAULT_CONFIG_PATH};

// Returns the required and generated params in default_config.json with the default value from the
// config presentation.
fn required_args() -> Vec<String> {
    let default_config = NodeConfig::default();
    let mut args = Vec::new();
    let mut config_presentation = get_config_presentation(&default_config, true).unwrap();

    for (param_path, serialized_param) in default_config.dump() {
        let serialization_type = match serialized_param.content {
            SerializedContent::DefaultValue(_) | SerializedContent::PointerTarget(_) => continue,
            SerializedContent::ParamType(serialization_type) => serialization_type,
        };
        args.push(format!("--{param_path}"));

        let required_param_json_value = param_path
            .split('.')
            .fold(&mut config_presentation, |entry, config_name| entry.index_mut(config_name));

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
    let mut default_config = NodeConfig::default();
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let dumped_default_config = default_config.dump();
    insta::assert_json_snapshot!(dumped_default_config);

    // The validate function will fail if the data directory does not exist so we change the path to
    // point to an existing directory.
    default_config.storage.db_config.path_prefix = PathBuf::from(".");
    default_config.validate().unwrap();
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

#[test]
fn default_config_file_is_up_to_date() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_CONFIG_PATH).unwrap()).unwrap();

    // Create a temporary file and dump the default config to it.
    let mut tmp_file_path = env::temp_dir();
    tmp_file_path.push("cfg.json");
    NodeConfig::default().dump_to_file(&CONFIG_POINTERS, tmp_file_path.to_str().unwrap()).unwrap();

    // Read the dumped config from the file.
    let from_code: serde_json::Value =
        serde_json::from_reader(File::open(tmp_file_path).unwrap()).unwrap();

    println!(
        "{}",
        "Default config file doesn't match the default NodeConfig implementation. Please update \
         it using the dump_config binary."
            .purple()
            .bold()
    );
    println!("Diffs shown below.");
    assert_json_eq!(from_default_config_file, from_code)
}
