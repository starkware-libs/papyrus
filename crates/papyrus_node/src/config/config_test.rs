use std::collections::{BTreeMap, HashMap};
use std::env::{self, args};
use std::io::Write;
use std::path::{Path, PathBuf};

use papyrus_config::command::update_config_map_by_command;
use papyrus_config::{SerdeConfig, SerializedParam};
use serde_json::{json, Map, Value};
use starknet_api::core::ChainId;
use tempfile::NamedTempFile;
use test_utils::get_absolute_path;

use super::dump_default_config_to_file;
use crate::config::{node_command, Config, ConfigBuilder};

const DEFAULT_CONFIG_FILE: &str = "config/default_config.json";

#[test]
fn load_default_config() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    // TODO(spapini): Move the config closer.
    Config::load_from_builder(vec![]).expect("Failed to load the config.");
}

#[test]
fn default_builder() {
    let builder = ConfigBuilder::default();
    assert_eq!(builder.config.gateway.chain_id, ChainId("SN_MAIN".to_owned()));
    assert!(builder.config.sync.is_some())
}

#[test]
fn prepare_command() {
    let args = vec![
        "Papyrus".to_owned(),
        "--config_file=conf.yaml".to_owned(),
        "--chain_id=CHAIN_ID".to_owned(),
        "--server_address=IP:PORT".to_owned(),
        "--http_headers=NAME_1:VALUE_1 NAME_2:VALUE_2".to_owned(),
        "--storage=path".to_owned(),
        "--no_sync=true".to_owned(),
        "--central_url=URL".to_owned(),
        "--collect_metrics=false".to_owned(),
    ];
    let builder = ConfigBuilder::default().prepare_command(args).unwrap();
    let builder_args = builder.args.expect("Expected to have args");

    assert_eq!(
        *builder_args.get_one::<PathBuf>("config_file").expect("Expected to have config arg"),
        PathBuf::from("conf.yaml")
    );
    assert_eq!(
        *builder_args.get_one::<String>("chain_id").expect("Expected to have chain_id arg"),
        "CHAIN_ID".to_owned()
    );
    assert_eq!(
        *builder_args
            .get_one::<String>("server_address")
            .expect("Expected to have server_address arg"),
        "IP:PORT".to_owned()
    );

    let headers_list: Vec<&str> = builder_args
        .get_one::<String>("http_headers")
        .expect("Expected to have http_headers args")
        .split(' ')
        .collect();
    assert_eq!(headers_list.len(), 2);
    assert_eq!(headers_list[0].to_owned(), "NAME_1:VALUE_1".to_owned());
    assert_eq!(headers_list[1].to_owned(), "NAME_2:VALUE_2".to_owned());

    assert_eq!(
        *builder_args.get_one::<PathBuf>("storage").expect("Expected to have storage arg"),
        PathBuf::from("path")
    );
    let no_sync = *builder_args.get_one::<bool>("no_sync").expect("Expected to have no_sync arg");
    assert!(no_sync);
    assert_eq!(
        *builder_args.get_one::<String>("central_url").expect("Expected to have central_url arg"),
        "URL".to_owned()
    );
    let collect_metrics = *builder_args
        .get_one::<bool>("collect_metrics")
        .expect("Expected to have collect_metrics arg");
    assert!(!collect_metrics);
}

#[test]
fn load_yaml_config() {
    let mut f = NamedTempFile::new().unwrap();
    let yaml = r"
chain_id: TEST
gateway:
    max_events_keys: 1234
";
    f.write_all(yaml.as_bytes()).unwrap();
    let args = vec!["Papyrus".to_owned(), format!("--config_file={}", f.path().to_str().unwrap())];
    let builder = ConfigBuilder::default().prepare_command(args).unwrap().yaml().unwrap();

    assert_eq!(builder.chain_id, ChainId("TEST".to_owned()));
    assert_eq!(builder.config.gateway.max_events_keys, 1234);
}

#[test]
fn load_http_headers() {
    let mut f = NamedTempFile::new().unwrap();
    let yaml = r"
central:
    http_headers:
        NAME_1: VALUE_1
        NAME_2: VALUE_2
";
    f.write_all(yaml.as_bytes()).unwrap();
    let args = vec![
        "Papyrus".to_owned(),
        format!("--config_file={}", f.path().to_str().unwrap()),
        "--http_headers=NAME_2:NEW_VALUE_2 NAME_3:VALUE_3".to_owned(),
    ];
    let builder =
        ConfigBuilder::default().prepare_command(args).unwrap().yaml().unwrap().args().unwrap();

    let target_http_headers = HashMap::from([
        ("NAME_1".to_string(), "VALUE_1".to_string()),
        ("NAME_2".to_string(), "NEW_VALUE_2".to_string()),
        ("NAME_3".to_string(), "VALUE_3".to_string()),
    ]);
    assert_eq!(builder.config.central.http_headers.unwrap(), target_http_headers);
}

#[test]
// Regression test which checks that the default config hasn't changed as well as dumping/parsing
// configs.
fn test_dump_default_config() {
    let dumped_default_config = Config::default().dump();
    insta::assert_json_snapshot!(dumped_default_config);

    let path = get_absolute_path(DEFAULT_CONFIG_FILE);
    let file = std::fs::File::open(path).unwrap();
    let deserialized_default_config: Map<String, Value> = serde_json::from_reader(file).unwrap();

    let mut deserialized_map: BTreeMap<String, SerializedParam> = BTreeMap::new();
    for (key, value) in deserialized_default_config {
        deserialized_map.insert(
            key.to_owned(),
            SerializedParam {
                description: value["description"].as_str().unwrap().to_owned(),
                value: value["value"].to_owned(),
            },
        );
    }

    assert_eq!(deserialized_map, dumped_default_config);
}

#[test]
fn test_dump_and_load() {
    let default_config = Config::default();
    let loaded_config = Config::load(&default_config.dump()).unwrap();
    assert_eq!(loaded_config, default_config);
}

#[test]
fn test_update_dumped_config_by_command() {
    let mut dumped_config = Config::default().dump();
    let args =
        vec!["Papyrus", "--gateway.max_events_keys", "1234", "--storage.db_config.path", "/abc"];
    update_config_map_by_command(&mut dumped_config, node_command(), args);
    assert_eq!(dumped_config.get("gateway.max_events_keys").unwrap().value, json!(1234));
    assert_eq!(dumped_config.get("storage.db_config.path").unwrap().value, json!("/abc"));
}
