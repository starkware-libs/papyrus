mod body;
mod header;
mod state;
#[cfg(test)]
mod test_utils;

use serde::{Deserialize, Serialize};

use crate::starknet::{
    BlockHash, BlockHeader, BlockNumber, ContractAddress, IndexedDeployedContract, StarkFelt,
    StateDiffForward, StorageKey, Transaction, TransactionIndex,
};
use crate::storage::db::{open_env, DbError, DbReader, DbWriter, TableIdentifier};
use crate::storage::db::{DbConfig, TableHandle};
use std::sync::Arc;

pub use self::header::{HeaderStorageReader, HeaderStorageWriter};

#[derive(thiserror::Error, Debug)]
pub enum BlockStorageError {
    #[error(transparent)]
    InnerError(#[from] DbError),
    #[error("Marker mismatch (expected {expected:?}, found {found:?}).")]
    MarkerMismatch {
        expected: BlockNumber,
        found: BlockNumber,
    },
    #[error(
        "Block hash {block_hash:?} already exists, when adding block number {block_number:?}."
    )]
    BlockHashAlreadyExists {
        block_hash: BlockHash,
        block_number: BlockNumber,
    },
    #[error("State diff redployed to an existing contract address {address:?}.")]
    ContractAlreadyExists { address: ContractAddress },
}
pub type BlockStorageResult<V> = std::result::Result<V, BlockStorageError>;

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum MarkerKind {
    Header,
    Body,
    State,
}
pub type MarkersTable<'env> = TableHandle<'env, MarkerKind, BlockNumber>;
pub struct Tables {
    markers: TableIdentifier<MarkerKind, BlockNumber>,
    headers: TableIdentifier<BlockNumber, BlockHeader>,
    block_hash_to_number: TableIdentifier<BlockHash, BlockNumber>,
    transactions: TableIdentifier<(BlockNumber, TransactionIndex), Transaction>,
    state_diffs: TableIdentifier<BlockNumber, StateDiffForward>,
    contracts: TableIdentifier<ContractAddress, IndexedDeployedContract>,
    contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockNumber), StarkFelt>,
}
#[derive(Clone)]
pub struct BlockStorageReader {
    db_reader: DbReader,
    tables: Arc<Tables>,
}
pub struct BlockStorageWriter {
    db_writer: DbWriter,
    tables: Arc<Tables>,
}

pub fn open_block_storage(
    db_config: DbConfig,
) -> BlockStorageResult<(BlockStorageReader, BlockStorageWriter)> {
    let (db_reader, mut db_writer) = open_env(db_config)?;
    let tables = Arc::new(Tables {
        markers: db_writer.create_table("markers")?,
        headers: db_writer.create_table("headers")?,
        block_hash_to_number: db_writer.create_table("block_hash_to_number")?,
        transactions: db_writer.create_table("transactions")?,
        state_diffs: db_writer.create_table("state_diffs")?,
        contracts: db_writer.create_table("contracts")?,
        contract_storage: db_writer.create_table("contract_storage")?,
    });
    let reader = BlockStorageReader {
        db_reader,
        tables: tables.clone(),
    };
    let writer = BlockStorageWriter { db_writer, tables };
    Ok((reader, writer))
}
