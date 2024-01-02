use std::collections::BTreeMap;
use std::env::args;
use std::io::Write;

use clap::Command;
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use papyrus_config::loading::load_and_process_config;
use papyrus_config::{ConfigError, ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{open_storage, StorageConfig};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use tempfile::NamedTempFile;

#[derive(Serialize, Debug, Default, Deserialize, Clone, PartialEq)]
struct Config {
    pub start_block: u64,
    pub end_block: Option<u64>,
    pub storage: StorageConfig,
}

impl SerializeConfig for Config {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dump = BTreeMap::from_iter([ser_param(
            "start_block",
            &self.start_block,
            "Start block",
            ParamPrivacyInput::Public,
        )]);
        dump.append(&mut ser_optional_param(
            &self.end_block,
            100,
            "end_block",
            "End block",
            ParamPrivacyInput::Public,
        ));
        dump.append(&mut append_sub_config_name(self.storage.dump(), "storage"));
        dump
    }
}

impl Config {
    pub fn load_and_process(args: Vec<String>) -> Self {
        let mut default_config_file = NamedTempFile::new().expect("Failed creating temp file");
        Config::default()
            .dump_to_file(&Vec::new(), default_config_file.path().to_str().unwrap())
            .expect("Failed dumping default config");
        default_config_file.flush().expect("Failed flushing temp file");
        let default_config_file = default_config_file.reopen().expect("Failed reopening temp file");
        let command = Command::new("StarknetVersions").about("Collects the StarkNet versions");
        match load_and_process_config::<Self>(default_config_file, command, args) {
            Ok(config) => config,
            Err(ConfigError::CommandInput(err)) => {
                err.exit();
            }
            Err(err) => {
                eprintln!("Failed loading config: {}", err);
                std::process::exit(1);
            }
        }
    }
}

fn main() {
    let args = args().collect();
    let config = Config::load_and_process(args);

    let (storage_reader, _) = open_storage(config.storage.clone()).expect("Failed opening storage");
    let txn = storage_reader.begin_ro_txn().expect("Failed opening read-only transaction");
    let to_block = config
        .end_block
        .unwrap_or_else(|| txn.get_header_marker().expect("Failed getting latest block number").0);
    let sn_versions = txn
        .get_startknet_versions(BlockNumber(config.start_block), BlockNumber(to_block))
        .expect("Failed getting StarkNet versions");
    println!("{sn_versions:#?}");
}
