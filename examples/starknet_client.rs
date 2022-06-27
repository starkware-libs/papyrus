use papyrus_lib::config::load_config;
use papyrus_lib::starknet::BlockNumber;
use papyrus_lib::starknet_client::StarknetClient;

#[tokio::main]
async fn main() {
    let config = load_config("config/config.ron").unwrap();
    let starknet_client = StarknetClient::new(&config.central.url).unwrap();
    let _latest_block_number = starknet_client.block_number().await.unwrap();
    let _block_header_123456 = starknet_client
        .block_header(BlockNumber(123456000))
        .await
        .unwrap();
    assert!(false);
    // TODO(dan): Add state_update once Starknet sequencer returns the class_hash in
    // get_state_update deployed_contracts prefixed.
}
