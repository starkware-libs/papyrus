use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use papyrus_storage::DbStats;

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets DB statistics.
    #[method(name = "dbStats")]
    fn db_stats(&self) -> Result<DbStats, Error>;
}
