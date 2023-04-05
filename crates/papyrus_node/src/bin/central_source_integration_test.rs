use std::{env, fs};

use futures_util::pin_mut;
use papyrus_node::config::Config;
use papyrus_storage::open_storage;
use papyrus_sync::{CentralSource, CentralSourceTrait};
use starknet_api::block::BlockNumber;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let mut path = env::temp_dir();
    path.push("data");
    fs::create_dir_all(path.clone()).expect("Make a temporary `data` directory");
    let config = Config::load(vec![
        "--chain_id=SN_GOERLI".to_owned(),
        "--central_url=https://external.integration.starknet.io/".to_owned(),
        format!("--storage={}", path.display()),
    ])
    .expect("Load config");
    let (storage_reader, _) = open_storage(config.storage.db_config).expect("Open storage");
    let central_source =
        CentralSource::new(config.central, storage_reader).expect("Create new client");
    let last_block_number = BlockNumber(283414);

    let mut block_marker = BlockNumber(283410);
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

    let mut state_marker = BlockNumber(283410);
    let header_stream = central_source.stream_state_updates(state_marker, last_block_number).fuse();
    pin_mut!(header_stream);
    while let Some(Ok((block_number, _block_hash, _state_difff, _deployed_classes))) =
        header_stream.next().await
    {
        assert!(
            state_marker == block_number,
            "Expected block number ({state_marker}) does not match the result ({block_number}).",
        );
        state_marker = state_marker.next();
    }
    assert!(state_marker == last_block_number);
}
