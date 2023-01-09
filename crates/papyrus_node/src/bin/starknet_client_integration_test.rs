use papyrus_node::config::Config;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::hash::StarkHash;
use starknet_client::{StarknetClient, StarknetClientTrait};

#[tokio::main]
async fn main() {
    let config = Config::load(vec![]).expect("Load config");
    let starknet_client =
        StarknetClient::new(&config.central.url, None, config.central.retry_config)
            .expect("Create new client");
    let _latest_block_number = starknet_client.block_number().await.expect("Get block number");
    // A block with invoke transaction version 1.
    let _block_376150 = starknet_client.block(BlockNumber(376150)).await.expect("Get block");
    // A block with deploy account transaction.
    let _block_376051 = starknet_client.block(BlockNumber(376051)).await.expect("Get block");
    // TODO(anatg): Write what's special in this block.
    let _block_1564 = starknet_client.block(BlockNumber(1564)).await.expect("Get block");
    let _block_123456 = starknet_client.block(BlockNumber(123456)).await.expect("Get block");
    let _state_diff =
        starknet_client.state_update(BlockNumber(123456)).await.expect("Get state diff");
    let class_hash = ClassHash(
        StarkHash::try_from("0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17")
            .unwrap(),
    );
    let _contract_class_by_hash =
        starknet_client.class_by_hash(class_hash).await.expect("Get class by hash");
}
