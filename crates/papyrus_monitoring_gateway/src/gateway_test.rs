use jsonrpsee::types::EmptyParams;
use papyrus_storage::{table_names, test_utils, DbTablesStats};

use super::api::JsonRpcServer;
use super::JsonRpcServerImpl;

#[tokio::test]
async fn test_stats() -> Result<(), anyhow::Error> {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage()?;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    let stats =
        module.call::<_, DbTablesStats>("starknet_dbTablesStats", EmptyParams::new()).await?;
    for &name in table_names() {
        assert!(stats.stats.contains_key(name))
    }
    Ok(())
}
