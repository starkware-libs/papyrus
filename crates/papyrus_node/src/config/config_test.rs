use std::env::{self, args};
use std::io::Write;
use std::path::{Path, PathBuf};

use starknet_api::core::ChainId;
use tempfile::NamedTempFile;

use crate::config::{Config, ConfigBuilder};

#[test]
fn load_default_config() {
    let workspace_root = Path::new("../../");
    env::set_current_dir(workspace_root).expect("Couldn't set working dir.");
    // TODO(spapini): Move the config closer.
    Config::load().expect("Failed to load the config.");
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
        "--config=conf.yaml".to_owned(),
        "--chain_id=CHAIN_ID".to_owned(),
        "--server_address=IP:PORT".to_owned(),
        "--storage=path".to_owned(),
        "--no_sync=true".to_owned(),
    ];
    let builder = ConfigBuilder::default().prepare_command(args).unwrap();
    let builder_args = builder.args.expect("Expected to have args");

    assert_eq!(
        *builder_args.get_one::<PathBuf>("config").expect("Expected to have config arg"),
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
gateway:
    max_events_keys: 100
";
    f.write_all(yaml.as_bytes()).unwrap();
    let args = vec!["Papyrus".to_owned(), format!("--config={}", f.path().to_str().unwrap())];
    let builder = ConfigBuilder::default().prepare_command(args).unwrap().yaml().unwrap();

    assert_eq!(builder.chain_id, ChainId("TEST".to_owned()));
    assert_eq!(builder.config.gateway.max_events_keys, 100);
}
