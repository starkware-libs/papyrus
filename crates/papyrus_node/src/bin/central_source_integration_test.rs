use futures_util::pin_mut;
use papyrus_node::config::load_config;
use papyrus_sync::CentralSource;
use starknet_api::BlockNumber;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let config = load_config("config/config.ron").expect("Load config");
    let central_source = CentralSource::new(config.central).expect("Create new client");
    let last_block_number = BlockNumber::new(203);

    let mut block_marker = BlockNumber::new(200);
    let block_stream = central_source.stream_new_blocks(block_marker, last_block_number).fuse();
    pin_mut!(block_stream);
    while let Some(Ok((block_number, _block))) = block_stream.next().await {
        assert!(
            block_marker == block_number,
            "Expected block number ({}) does not match the result ({}).",
            block_marker,
            block_number
        );
        block_marker = block_marker.next();
    }
    assert!(block_marker == last_block_number);

    let mut state_marker = BlockNumber::new(200);
    let header_stream = central_source.stream_state_updates(state_marker, last_block_number).fuse();
    pin_mut!(header_stream);
    while let Some(Ok((block_number, _state_difff, _deployed_classes))) = header_stream.next().await
    {
        assert!(
            state_marker == block_number,
            "Expected block number ({}) does not match the result ({}).",
            state_marker,
            block_number
        );
        state_marker = state_marker.next();
    }
    assert!(state_marker == last_block_number);
}
