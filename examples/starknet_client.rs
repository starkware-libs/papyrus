use papyrus_lib::config::load_config;
use papyrus_lib::starknet::BlockNumber;
use papyrus_lib::starknet_client::StarknetClient;

#[tokio::main]
async fn main() {
    let config = load_config("config/config.ron").unwrap();
    let starknet_client = StarknetClient::new(&config.central.url).unwrap();
    let _latest_block_number = starknet_client.block_number().await.unwrap();
    let _block_123456 = starknet_client.block(BlockNumber(123456)).await.unwrap();
    let _state_diff = starknet_client.state_update(BlockNumber(123456)).await;
}
