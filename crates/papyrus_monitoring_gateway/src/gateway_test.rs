use jsonrpsee::types::EmptyParams;
use papyrus_storage::{test_utils, DbStats, TABLE_NAMES};

use super::api::JsonRpcServer;
use super::JsonRpcServerImpl;

#[tokio::test]
async fn test_stats() -> Result<(), anyhow::Error> {
    let (storage_reader, mut _storage_writer) = test_utils::get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    let stats = module.call::<_, DbStats>("starknet_dbStats", EmptyParams::new()).await?;
    for name in TABLE_NAMES {
        assert!(stats.stats.contains_key(name))
    }
    Ok(())
}
