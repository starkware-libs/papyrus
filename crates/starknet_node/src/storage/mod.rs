#[path = "components/body.rs"]
mod body;
mod db;
#[path = "components/header.rs"]
mod header;
#[path = "components/state.rs"]
mod state;
#[cfg(test)]
#[path = "test_utils.rs"]
pub mod test_utils;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, BlockHeader, BlockNumber, ContractAddress, IndexedDeployedContract, StarkFelt,
    StateDiffForward, StorageKey, Transaction, TransactionHash, TransactionOffsetInBlock,
};

pub use self::body::{BodyStorageReader, BodyStorageWriter};
pub use self::db::TransactionKind;
use self::db::{
    open_env, DbConfig, DbError, DbReader, DbTransaction, DbWriter, TableHandle, TableIdentifier,
    RO, RW,
};
pub use self::header::{HeaderStorageReader, HeaderStorageWriter};
pub use self::state::{StateStorageReader, StateStorageWriter};

#[derive(Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_config: DbConfig,
}

pub struct StorageComponents {
    pub block_storage_reader: BlockStorageReader,
    pub block_storage_writer: BlockStorageWriter,
}

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error(transparent)]
    BlockStorageError(#[from] BlockStorageError),
}

impl StorageComponents {
    pub fn new(config: StorageConfig) -> Result<Self, StorageError> {
        let (block_storage_reader, block_storage_writer) = open_block_storage(config.db_config)?;
        Ok(Self { block_storage_reader, block_storage_writer })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BlockStorageError {
    #[error(transparent)]
    InnerError(#[from] DbError),
    #[error("Marker mismatch (expected {expected:?}, found {found:?}).")]
    MarkerMismatch { expected: BlockNumber, found: BlockNumber },
    #[error(
        "Block hash {block_hash:?} already exists, when adding block number {block_number:?}."
    )]
    BlockHashAlreadyExists { block_hash: BlockHash, block_number: BlockNumber },
    #[error(
        "Transaction hash {tx_hash:?} already exists, when adding transaction \
         {tx_offset_in_block:?} at block number {block_number:?}."
    )]
    TransactionHashAlreadyExists {
        tx_hash: TransactionHash,
        block_number: BlockNumber,
        tx_offset_in_block: TransactionOffsetInBlock,
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
    transactions: TableIdentifier<(BlockNumber, TransactionOffsetInBlock), Transaction>,
    transaction_hash_to_idx:
        TableIdentifier<TransactionHash, (BlockNumber, TransactionOffsetInBlock)>,
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
pub struct BlockStorageTxn<'env, Mode: TransactionKind> {
    txn: DbTransaction<'env, Mode>,
    tables: Arc<Tables>,
}
impl BlockStorageReader {
    pub fn begin_ro_txn(&self) -> BlockStorageResult<BlockStorageTxn<'_, RO>> {
        Ok(BlockStorageTxn { txn: self.db_reader.begin_ro_txn()?, tables: self.tables.clone() })
    }
}
impl BlockStorageWriter {
    pub fn begin_rw_txn(&mut self) -> BlockStorageResult<BlockStorageTxn<'_, RW>> {
        Ok(BlockStorageTxn { txn: self.db_writer.begin_rw_txn()?, tables: self.tables.clone() })
    }
}
impl<'env> BlockStorageTxn<'env, RW> {
    pub fn commit(self) -> BlockStorageResult<()> {
        Ok(self.txn.commit()?)
    }
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
        transaction_hash_to_idx: db_writer.create_table("transaction_hash_to_idx")?,
        state_diffs: db_writer.create_table("state_diffs")?,
        contracts: db_writer.create_table("contracts")?,
        contract_storage: db_writer.create_table("contract_storage")?,
    });
    let reader = BlockStorageReader { db_reader, tables: tables.clone() };
    let writer = BlockStorageWriter { db_writer, tables };
    Ok((reader, writer))
}
