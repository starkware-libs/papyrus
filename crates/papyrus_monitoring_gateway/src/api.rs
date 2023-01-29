use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use papyrus_storage::DbTablesStats;
use serde_json::Value;

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets DB statistics.
    #[method(name = "dbTablesStats")]
    fn db_tables_stats(&self) -> Result<DbTablesStats, Error>;

    /// Gets the node config.
    #[method(name = "nodeConfig")]
    fn node_config(&self) -> Result<Value, Error>;
}
