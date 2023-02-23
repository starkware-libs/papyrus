use std::collections::HashMap;
use std::env::{self, args};
use std::io::Write;
use std::path::{Path, PathBuf};

use starknet_api::core::ChainId;
use tempfile::NamedTempFile;
use test_utils::get_absolute_path;

use crate::config::{Config, ConfigBuilder};

#[test]
fn load_default_config() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    // TODO(spapini): Move the config closer.
    Config::load(vec![]).expect("Failed to load the config.");
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
}

#[test]
fn load_yaml_config() {
    let mut f = NamedTempFile::new().unwrap();
    let yaml = r"
chain_id: TEST
central:
    concurrent_requests: 50
    retry:
        max_retries: 30
gateway:
    server_address: 0.0.0.0:5000
    max_events_keys: 1234

monitoring_gateway:
    server_address: 0.0.0.0:5001

storage:
    db:
        path: ./path/to/db 
";
    f.write_all(yaml.as_bytes()).unwrap();
    let args = vec!["Papyrus".to_owned(), format!("--config_file={}", f.path().to_str().unwrap())];
    let config = Config::load(args).unwrap();

    // Gateway configuration tests
    assert_eq!(config.gateway.chain_id, ChainId("TEST".to_owned()));
    assert_eq!(config.gateway.max_events_keys, 1234);

    // Central configuration tests
    assert_eq!(config.central.concurrent_requests, 50);
    assert_eq!(config.central.retry_config.max_retries, 30);

    // Db configuration tests
    assert_eq!(config.storage.db_config.path, String::from("./path/to/db"))
}

#[test]
fn env_over_yaml_precedence() {
    let mut f = NamedTempFile::new().unwrap();
    let yaml = r"
chain_id: TEST
gateway:
    server_address: 0.0.0.0:8080
storage:
    db:
        path: ./path-must-be-overriden-by-env 
";
    f.write_all(yaml.as_bytes()).unwrap();
    let args = vec!["Papyrus".to_owned(), format!("--config_file={}", f.path().to_str().unwrap())];
    std::env::set_var("PAPYRUS_GATEWAY_SERVER_ADDRESS", "0.0.0.0:5000");
    std::env::set_var("PAPYRUS_STORAGE_DB_PATH", "./path-to-db");
    let config = Config::load(args).unwrap();

    // Gateway configuration tests
    assert_eq!(config.gateway.chain_id, ChainId("TEST".to_owned()));
    assert_eq!(config.gateway.server_address, String::from("0.0.0.0:5000"));
    assert_eq!(config.storage.db_config.path, String::from("./path-to-db"));
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
    let config = Config::load(args).unwrap();

    let target_http_headers = HashMap::from([
        ("NAME_1".to_string(), "VALUE_1".to_string()),
        ("NAME_2".to_string(), "NEW_VALUE_2".to_string()),
        ("NAME_3".to_string(), "VALUE_3".to_string()),
    ]);
    assert_eq!(config.central.http_headers.unwrap(), target_http_headers);
}
