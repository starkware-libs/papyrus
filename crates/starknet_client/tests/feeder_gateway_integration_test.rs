#![allow(clippy::unwrap_used)]
use serde::Serialize;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::hash::StarkHash;
use starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};
use starknet_client::retry::RetryConfig;
use tokio::join;

const NODE_VERSION: &str = "PAPYRUS-INTEGRATION-TEST-STARKNET-FEEDER-GATEWAY-CLIENT";

#[derive(Serialize)]
// Blocks with API changes to be tested with the get_block function.
struct BlocksForGetBlock {
    first_block: Option<u32>,
    // Each field is a block number where a transaction from the given type is in.
    invoke_version_0: Option<u32>,
    invoke_version_1: Option<u32>,
    invoke_version_3: Option<u32>,
    declare_version_0: Option<u32>,
    declare_version_1: Option<u32>,
    declare_version_2: Option<u32>,
    declare_version_3: Option<u32>,
    deploy_account_version_1: Option<u32>,
    deploy_account_version_3: Option<u32>,
    deploy: Option<u32>,
    l1_handler: Option<u32>,
}

#[derive(Serialize)]
// Blocks with API changes to be tested with the get_state_update function.
struct BlocksForGetStateUpdate {
    first_block: Option<u32>,
    // A state update with 'old_declared_contracts'. (added in v0.9.1).
    old_declared_contracts: Option<u32>,
    // A state update with 'nonces'. (added in v0.10.0).
    nonces: Option<u32>,
    // A state update with 'declared_classes'. (added in v0.11.0).
    declared_classes: Option<u32>,
    // A state update with 'replaced_classes'. (added in v0.11.0).
    replaced_classes: Option<u32>,
}

#[derive(Serialize)]
// Class hashes of different versions.
struct ClassHashes {
    // A class definition of Cairo 0 contract.
    cairo_0_class_hash: String,
    // A class definition of Cairo 1 contract. (added in v0.11.0).
    cairo_1_class_hash: String,
}

// Test data for a specific testnet.
struct TestEnvData {
    url: String,
    get_blocks: BlocksForGetBlock,
    get_state_updates: BlocksForGetStateUpdate,
    class_hashes: ClassHashes,
}

fn into_block_number_vec<T: Serialize>(obj: T) -> Vec<BlockNumber> {
    serde_json::to_value(obj)
        .unwrap()
        .as_object()
        .unwrap()
        .values()
        .filter(|val| !val.is_null())
        .map(|block_number_json_val| BlockNumber(block_number_json_val.as_u64().unwrap()))
        .collect()
}

#[tokio::test]
#[ignore]
async fn test_mainnet() {
    let _ = simple_logger::init_with_env();
    let mainnet_data = TestEnvData {
        url: "https://alpha-mainnet.starknet.io/".to_owned(),
        get_blocks: BlocksForGetBlock {
            first_block: Some(0),
            invoke_version_0: None, // Include in block 0
            invoke_version_1: Some(636864),
            invoke_version_3: None, // Include in block 636864
            declare_version_0: Some(2700),
            declare_version_1: Some(346864),
            declare_version_2: Some(446864),
            declare_version_3: Some(630723),
            deploy_account_version_1: None, // Include in block 636864
            deploy_account_version_3: None, // Include in block 636864
            deploy: None,                   // Include in block 0
            l1_handler: None,               // Include in block 630723
        },
        get_state_updates: BlocksForGetStateUpdate {
            first_block: Some(0),
            old_declared_contracts: Some(351506),
            nonces: None,           // Include in block 351506
            declared_classes: None, // Include in block 351506
            replaced_classes: None, // Include in block 351506
        },
        class_hashes: ClassHashes {
            cairo_0_class_hash: "0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
                .to_owned(),
            cairo_1_class_hash: "0x7f3777c99f3700505ea966676aac4a0d692c2a9f5e667f4c606b51ca1dd3420"
                .to_owned(),
        },
    };
    run(mainnet_data).await;
}

#[tokio::test]
#[ignore]
async fn test_sepolia_integration() {
    let _ = simple_logger::init_with_env();
    let mainnet_data = TestEnvData {
        url: "https://integration-sepolia.starknet.io/".to_owned(),
        get_blocks: BlocksForGetBlock {
            first_block: Some(0),
            invoke_version_0: None, // Was deprecated before the chain started
            invoke_version_1: None, // in 0
            invoke_version_3: Some(20000),
            declare_version_0: None, // Include in block 0
            declare_version_1: Some(23),
            declare_version_2: None, // Include in block 23
            declare_version_3: Some(26972),
            deploy_account_version_1: None, // Include in block 0
            deploy_account_version_3: Some(28364),
            deploy: None,     // Was deprecated before the chain started
            l1_handler: None, // Include in block 0
        },
        get_state_updates: BlocksForGetStateUpdate {
            first_block: Some(0),
            old_declared_contracts: None, // Include in block 0
            nonces: None,                 // Include in block 0
            declared_classes: Some(1795),
            replaced_classes: Some(1798),
        },
        class_hashes: ClassHashes {
            cairo_0_class_hash: "0x5c478ee27f2112411f86f207605b2e2c58cdb647bac0df27f660ef2252359c6"
                .to_owned(),
            cairo_1_class_hash: "0x93628b393a98e858c0038175f6c44cb5bca1132a765273ad10dcf7d756537f"
                .to_owned(),
        },
    };
    run(mainnet_data).await;
}

async fn run(test_env_data: TestEnvData) {
    let starknet_client = StarknetFeederGatewayClient::new(
        &test_env_data.url,
        None,
        NODE_VERSION,
        RetryConfig { retry_base_millis: 30, retry_max_delay_millis: 30000, max_retries: 10 },
    )
    .expect("Create new client");

    join!(
        test_get_block(&starknet_client, test_env_data.get_blocks),
        test_get_state_update(&starknet_client, test_env_data.get_state_updates),
        test_class_hash(&starknet_client, test_env_data.class_hashes),
        async { starknet_client.pending_data().await.unwrap().unwrap() },
    );
}

// Call get_block on the given list of block_numbers.
async fn test_get_block(
    starknet_client: &StarknetFeederGatewayClient,
    block_numbers: BlocksForGetBlock,
) {
    for block_number in into_block_number_vec(block_numbers) {
        starknet_client.block(block_number).await.unwrap().unwrap();
    }

    // Get the last block.
    starknet_client.latest_block().await.unwrap().unwrap();
    // Not existing block.
    assert!(starknet_client.block(BlockNumber(u64::MAX)).await.unwrap().is_none());
}

// Call get_state_update on the given list of block_numbers.
async fn test_get_state_update(
    starknet_client: &StarknetFeederGatewayClient,
    block_numbers: BlocksForGetStateUpdate,
) {
    for block_number in into_block_number_vec(block_numbers) {
        starknet_client.state_update(block_number).await.unwrap().unwrap();
    }
}

// Call class_by_hash for the given list of class_hashes.
async fn test_class_hash(starknet_client: &StarknetFeederGatewayClient, class_hashes: ClassHashes) {
    let data = serde_json::to_value(class_hashes).unwrap();

    for class_hash_json_val in data.as_object().unwrap().values() {
        let class_hash_val = class_hash_json_val.as_str().unwrap();
        let class_hash = ClassHash(StarkHash::try_from(class_hash_val).unwrap());
        starknet_client.class_by_hash(class_hash).await.unwrap().unwrap();
    }
}
