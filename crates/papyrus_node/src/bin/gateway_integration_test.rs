use std::env;
use std::fs::{read_to_string, remove_dir_all};
use std::path::Path;
use std::process::Command;
use std::string::String;

use futures_util::pin_mut;
use papyrus_gateway::run_server;
use papyrus_node::config::load_config;
use papyrus_storage::{open_storage, BodyStorageWriter, HeaderStorageWriter, StateStorageWriter};
use papyrus_sync::CentralSource;
use starknet_api::BlockNumber;
use tokio_stream::StreamExt;

const ARGS: &str = r#"-s,-X,POST,-H,Content-Type: application/json,http://localhost:8080,-d"#;

fn read_resource_file(path_in_resource_dir: &str) -> String {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    return read_to_string(path.to_str().unwrap()).unwrap().replace('\n', "").replace(' ', "");
}

fn get_args(method: &str, params: &str) -> Vec<String> {
    let new_arg =
        format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{}","params":[{{{}}}]}}"#, method, params);
    let mut v: Vec<String> = ARGS.split(',').map(String::from).collect();
    v.push(new_arg);
    v
}

#[tokio::main]
async fn main() {
    let config = load_config("config/config.ron").expect("Load config");
    let central_source = CentralSource::new(config.central).unwrap();
    remove_dir_all(config.storage.db_config.path.clone()).unwrap();
    let (storage_reader, mut storage_writer) = open_storage(config.storage.db_config).unwrap();

    let last_block_number = BlockNumber(10);
    let mut block_marker = BlockNumber(0);
    let block_stream = central_source.stream_new_blocks(block_marker, last_block_number).fuse();
    pin_mut!(block_stream);
    while let Some(Ok((block_number, block))) = block_stream.next().await {
        storage_writer
            .begin_rw_txn()
            .unwrap()
            .append_header(block_number, &block.header)
            .unwrap()
            .append_body(block_number, &block.body)
            .unwrap()
            .commit()
            .unwrap();
        block_marker = block_marker.next();
    }

    let mut state_marker = BlockNumber(0);
    let state_stream = central_source.stream_state_updates(state_marker, last_block_number).fuse();
    pin_mut!(state_stream);
    while let Some(Ok((block_number, state_diff))) = state_stream.next().await {
        storage_writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(block_number, state_diff)
            .unwrap()
            .commit()
            .unwrap();
        state_marker = state_marker.next();
    }

    let _gateway_thread =
        tokio::spawn(async move { run_server(config.gateway, storage_reader).await });

    let output = Command::new("curl").args(get_args("starknet_blockNumber", "")).output().unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        r#"{"jsonrpc":"2.0","result":9,"id":"1"}"#
    );

    let output = Command::new("curl")
        .args(get_args("starknet_getBlockWithTxs", r#""block_number": 1"#))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        read_resource_file("block_with_transactions.json")
    );

    let output = Command::new("curl")
        .args(get_args(
            "starknet_getBlockWithTxHashes",
            r#""block_hash": "0x11172ea58125f54df2c07df73accd9236558944ec0ee650d80968f863267764""#,
        ))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        read_resource_file("block_with_transaction_hashes.json")
    );

    let output = Command::new("curl")
        .args(get_args("starknet_getStateUpdate", r#""block_number": 9"#))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(output.stdout).unwrap(), read_resource_file("state_update.json"));
}
