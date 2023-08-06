use papyrus_node::config::NodeConfig;
use papyrus_node::version::VERSION_FULL;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::hash::StarkHash;
use starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};

#[tokio::main]
async fn main() {
    let config = NodeConfig::load_and_process(vec![
        "--chain_id=SN_GOERLI".to_owned(),
        "--central.url=https://alpha4.starknet.io/".to_owned(),
    ])
    .expect("Load config");
    let starknet_client = StarknetFeederGatewayClient::new(
        &config.central.url,
        None,
        VERSION_FULL,
        config.central.retry_config,
    )
    .expect("Create new client");

    // Get the last block.
    // Last block.
    starknet_client.latest_block().await.expect("").unwrap();

    // Get a block.
    // First block, the original definitions.
    starknet_client.block(BlockNumber(0)).await.expect("").unwrap();
    // A block with declare transaction. (added in v0.9.0).
    starknet_client.block(BlockNumber(248971)).await.expect("").unwrap();
    // A block with starknet version. (added in v0.9.1).
    starknet_client.block(BlockNumber(280000)).await.expect("").unwrap();
    // A block with declare transaction version 1. (added in v0.10.0).
    // A block with nonce field in transaction. (added in v0.10.0).
    starknet_client.block(BlockNumber(330039)).await.expect("").unwrap();
    // A block with invoke_function transaction version 1 (added in v0.10.0).
    starknet_client.block(BlockNumber(330291)).await.expect("").unwrap();
    // A block with deploy_account transaction. (added in v0.10.1).
    starknet_client.block(BlockNumber(385429)).await.expect("").unwrap();
    // A block with declare transaction version 2. (added in v0.11.0).
    starknet_client.block(BlockNumber(789048)).await.expect("").unwrap();
    // Not existing block.
    assert!(starknet_client.block(BlockNumber(u64::MAX)).await.expect("").is_none());

    // Get a state update.
    // First block, the original definitions.
    starknet_client.state_update(BlockNumber(0)).await.expect("").unwrap();
    // A state update with 'old_declared_contracts'. (added in v0.9.1).
    starknet_client.state_update(BlockNumber(248971)).await.expect("").unwrap();
    // A state update with 'nonces'. (added in v0.10.0).
    starknet_client.state_update(BlockNumber(330039)).await.expect("").unwrap();
    // A state update with 'declared_classes'. (added in v0.11.0).
    starknet_client.state_update(BlockNumber(788504)).await.expect("").unwrap();
    // A state update with 'replaced_classes'. (added in v0.11.0).
    starknet_client.state_update(BlockNumber(789048)).await.expect("").unwrap();

    // Get a class by hash.
    // A Cairo 0 class hash.
    let class_hash = ClassHash(
        StarkHash::try_from("0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17")
            .unwrap(),
    );
    // A class definition of Cairo 0 contract.
    starknet_client.class_by_hash(class_hash).await.expect("").unwrap();
    // A Cairo 1 class hash.
    let class_hash = ClassHash(
        StarkHash::try_from("0x702a9e80c74a214caf0e77326180e72ba3bd3f53dbd5519ede339eb3ae9eed4")
            .unwrap(),
    );
    // A class definition of Cairo 1 contract. (added in v0.11.0).
    starknet_client.class_by_hash(class_hash).await.expect("").unwrap();
}
