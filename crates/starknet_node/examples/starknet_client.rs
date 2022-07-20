use starknet_api::BlockNumber;
use starknet_client::StarknetClient;
use starknet_node::config::load_config;

#[tokio::main]
async fn main() {
    let config = load_config("config/config.ron").expect("Load config");
    let starknet_client = StarknetClient::new(&config.central.url).expect("Create new client");
    let _latest_block_number = starknet_client.block_number().await.expect("Get block number");
    let _block_123456 = starknet_client.block(BlockNumber(123456)).await.expect("Get block");
    let _state_diff =
        starknet_client.state_update(BlockNumber(123456)).await.expect("Get state diff");
}
