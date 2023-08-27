use std::collections::{BTreeMap, HashMap};
use std::env::{self, args};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use papyrus_config::dumping::SerializeConfig;
use papyrus_config::SerializedParam;
use pretty_assertions::assert_eq;
use serde_json::{json, Map, Value};
use starknet_api::core::ChainId;
use tempfile::NamedTempFile;
use test_utils::get_absolute_path;
use validator::Validate;

use crate::config::{node_command, NodeConfig, DEFAULT_CONFIG_PATH};

// Fill here all the required params in default_config.json with some default value.
fn required_args() -> Vec<String> {
    vec![]
}

#[test]
fn load_default_config() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    NodeConfig::load_and_process(required_args()).expect("Failed to load the config.");
}

#[test]
fn load_http_headers() {
    let args = vec!["Papyrus", "--central.http_headers", "NAME_1:VALUE_1 NAME_2:VALUE_2"];
    let mut args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();
    args.append(&mut required_args());

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
    default_config.validate().unwrap();
    let dumped_default_config = default_config.dump();
    insta::assert_json_snapshot!(dumped_default_config);
}

#[test]
fn test_default_config_process() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    assert_eq!(NodeConfig::load_and_process(required_args()).unwrap(), NodeConfig::default());
}

#[test]
fn test_update_dumped_config_by_command() {
    let args =
        vec!["Papyrus", "--rpc.max_events_keys", "1234", "--storage.db_config.path_prefix", "/abc"];
    let mut args: Vec<String> = args.into_iter().map(|s| s.to_owned()).collect();
    args.append(&mut required_args());
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let config = NodeConfig::load_and_process(args).unwrap();

    assert_eq!(config.rpc.max_events_keys, 1234);
    assert_eq!(config.storage.db_config.path_prefix.to_str(), Some("/abc"));
}
