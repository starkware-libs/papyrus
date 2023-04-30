use std::{env, fs};

use futures_util::pin_mut;
use papyrus_node::config::Config;
use papyrus_node::version::VERSION_FULL;
use papyrus_storage::open_storage;
use papyrus_sync::{CentralSource, CentralSourceTrait};
use starknet_api::block::BlockNumber;
use tokio_stream::StreamExt;

const STREAM_LENGTH: u64 = 10;

#[tokio::main]
async fn main() {
    let mut path = env::temp_dir();
    path.push("data");
    let _ = fs::remove_dir_all(path.clone());
    fs::create_dir_all(path.clone()).expect("Should make a temporary `data` directory");
    let config = Config::load(vec![
        "--chain_id=SN_GOERLI".to_owned(),
        "--central_url=https://alpha4.starknet.io/".to_owned(),
        format!("--storage={}", path.display()),
    ])
    .expect("Load config");
    let (storage_reader, _) = open_storage(config.storage).expect("Open storage");
    let central_source = CentralSource::new(config.central, VERSION_FULL, storage_reader)
        .expect("Create new client");
    let last_block_number =
        central_source.get_block_marker().await.expect("Central get block marker");
    let initial_block_number = BlockNumber(last_block_number.0 - STREAM_LENGTH);

    let mut block_marker = initial_block_number;
    let block_stream = central_source.stream_new_blocks(block_marker, last_block_number).fuse();
    pin_mut!(block_stream);
    while let Some(Ok((block_number, _block))) = block_stream.next().await {
        assert!(
            block_marker == block_number,
            "Expected block number ({block_marker}) does not match the result ({block_number}).",
        );
        block_marker = block_marker.next();
    }
    assert!(block_marker == last_block_number);

    let mut state_marker = initial_block_number;
    let state_update_stream =
        central_source.stream_state_updates(state_marker, last_block_number).fuse();
    pin_mut!(state_update_stream);
    while let Some(Ok((block_number, _block_hash, _state_diff, _deployed_classes))) =
        state_update_stream.next().await
    {
        assert!(
            state_marker == block_number,
            "Expected block number ({state_marker}) does not match the result ({block_number}).",
        );
        state_marker = state_marker.next();
    }
    assert!(state_marker == last_block_number);
}
