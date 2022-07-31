use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{BlockNumber, ClassHash, StarkHash};
use starknet_client::{StarknetClient, StarknetClientTrait};
use starknet_node::config::load_config;

#[tokio::main]
async fn main() {
    let config = load_config("config/config.ron").expect("Load config");
    let starknet_client = StarknetClient::new(&config.central.url).expect("Create new client");
    let _latest_block_number = starknet_client.block_number().await.expect("Get block number");
    let _block_123456 = starknet_client.block(BlockNumber(123456)).await.expect("Get block");
    let _state_diff =
        starknet_client.state_update(BlockNumber(123456)).await.expect("Get state diff");
    let _contract_class_by_hash = starknet_client
        .class_by_hash(ClassHash(StarkHash(
            bytes_from_hex_str::<32, true>(
                "0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17",
            )
            .unwrap(),
        )))
        .await
        .expect("Get class by hash");
}
