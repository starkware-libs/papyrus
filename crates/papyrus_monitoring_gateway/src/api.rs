use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use papyrus_storage::DbTablesStats;

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets DB statistics.
    #[method(name = "dbTablesStats")]
    fn db_tables_stats(&self) -> Result<DbTablesStats, Error>;
}
