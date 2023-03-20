use jsonrpsee::types::EmptyParams;
use papyrus_storage::{table_names, test_utils, DbTablesStats};

use super::api::JsonRpcServer;
use super::JsonRpcServerImpl;

const TEST_CONFIG_REPRESENTATION: &str = "general_config_representation";
const TEST_VERSION: &str = "1.2.3-dev";

#[tokio::test]
async fn test_stats() {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage();
    let module = JsonRpcServerImpl {
        storage_reader,
        general_config_representation: serde_yaml::to_value(TEST_CONFIG_REPRESENTATION).unwrap(),
        version: TEST_VERSION,
    }
    .into_rpc();
    let stats = module
        .call::<_, DbTablesStats>("starknet_dbTablesStats", EmptyParams::new())
        .await
        .expect("Monitoring gateway should respond with DB table statistics");
    for &name in table_names() {
        assert!(stats.stats.contains_key(name))
    }
}

#[tokio::test]
async fn test_config() {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage();
    let module = JsonRpcServerImpl {
        storage_reader,
        general_config_representation: serde_yaml::to_value(TEST_CONFIG_REPRESENTATION).unwrap(),
        version: TEST_VERSION,
    }
    .into_rpc();
    let rep = module
        .call::<_, String>("starknet_nodeConfig", EmptyParams::new())
        .await
        .expect("Monitoring gateway should respond the node configuration");
    assert_eq!(rep, TEST_CONFIG_REPRESENTATION);
}

#[tokio::test]
async fn test_version() {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage();
    let module = JsonRpcServerImpl {
        storage_reader,
        general_config_representation: serde_yaml::to_value(TEST_CONFIG_REPRESENTATION).unwrap(),
        version: TEST_VERSION,
    }
    .into_rpc();
    let rep = module
        .call::<_, String>("starknet_nodeVersion", EmptyParams::new())
        .await
        .expect("Monitoring gateway should respond with the node's version");
    assert_eq!(rep, TEST_VERSION);
}
