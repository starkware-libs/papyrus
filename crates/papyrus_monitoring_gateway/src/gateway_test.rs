use jsonrpsee::types::EmptyParams;
use papyrus_storage::{table_names, test_utils, DbTablesStats};

use super::api::JsonRpcServer;
use super::JsonRpcServerImpl;

const TEST_CONFIG_REPRESENTATION: &str = "general_config_representation";

#[tokio::test]
async fn test_stats() -> Result<(), anyhow::Error> {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage();
    let module = JsonRpcServerImpl {
        storage_reader,
        general_config_representation: serde_yaml::to_value(TEST_CONFIG_REPRESENTATION)?,
    }
    .into_rpc();
    let stats =
        module.call::<_, DbTablesStats>("starknet_dbTablesStats", EmptyParams::new()).await?;
    for &name in table_names() {
        assert!(stats.stats.contains_key(name))
    }
    Ok(())
}

#[tokio::test]
async fn test_config() -> Result<(), anyhow::Error> {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage();
    let module = JsonRpcServerImpl {
        storage_reader,
        general_config_representation: serde_yaml::to_value(TEST_CONFIG_REPRESENTATION)?,
    }
    .into_rpc();
    let rep = module.call::<_, String>("starknet_nodeConfig", EmptyParams::new()).await?;
    assert_eq!(rep, TEST_CONFIG_REPRESENTATION);
    Ok(())
}
