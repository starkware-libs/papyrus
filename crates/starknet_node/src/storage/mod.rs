mod body;
mod db;
mod header;
mod state;
#[cfg(test)]
#[path = "test_utils.rs"]
pub mod test_utils;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, BlockHeader, BlockNumber, ClassHash, ContractAddress, IndexedDeclaredContract,
    IndexedDeployedContract, StarkFelt, StorageKey, Transaction, TransactionHash,
    TransactionOffsetInBlock,
};

pub use self::body::{BodyStorageReader, BodyStorageWriter};
pub use self::db::TransactionKind;
use self::db::{
    open_env, DbConfig, DbError, DbReader, DbTransaction, DbWriter, TableHandle, TableIdentifier,
    RO, RW,
};
pub use self::header::{HeaderStorageReader, HeaderStorageWriter};
pub use self::state::{StateStorageReader, StateStorageWriter, ThinStateDiff};

#[derive(Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_config: DbConfig,
}

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
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
    #[error(
        "State diff redeclared a different class to an existing contract hash {class_hash:?}."
    )]
    ClassAlreadyExists { class_hash: ClassHash },
}
pub type StorageResult<V> = std::result::Result<V, StorageError>;

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
    state_diffs: TableIdentifier<BlockNumber, ThinStateDiff>,
    declared_classes: TableIdentifier<ClassHash, IndexedDeclaredContract>,
    deployed_contracts: TableIdentifier<ContractAddress, IndexedDeployedContract>,
    contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockNumber), StarkFelt>,
}
#[derive(Clone)]
pub struct StorageReader {
    db_reader: DbReader,
    tables: Arc<Tables>,
}
pub struct StorageWriter {
    db_writer: DbWriter,
    tables: Arc<Tables>,
}
pub struct StorageTxn<'env, Mode: TransactionKind> {
    txn: DbTransaction<'env, Mode>,
    tables: Arc<Tables>,
}
impl StorageReader {
    pub fn begin_ro_txn(&self) -> StorageResult<StorageTxn<'_, RO>> {
        Ok(StorageTxn { txn: self.db_reader.begin_ro_txn()?, tables: self.tables.clone() })
    }
}
impl StorageWriter {
    pub fn begin_rw_txn(&mut self) -> StorageResult<StorageTxn<'_, RW>> {
        Ok(StorageTxn { txn: self.db_writer.begin_rw_txn()?, tables: self.tables.clone() })
    }
}
impl<'env> StorageTxn<'env, RW> {
    pub fn commit(self) -> StorageResult<()> {
        Ok(self.txn.commit()?)
    }
}

pub fn open_storage(db_config: DbConfig) -> StorageResult<(StorageReader, StorageWriter)> {
    let (db_reader, mut db_writer) = open_env(db_config)?;
    let tables = Arc::new(Tables {
        markers: db_writer.create_table("markers")?,
        headers: db_writer.create_table("headers")?,
        block_hash_to_number: db_writer.create_table("block_hash_to_number")?,
        transactions: db_writer.create_table("transactions")?,
        transaction_hash_to_idx: db_writer.create_table("transaction_hash_to_idx")?,
        state_diffs: db_writer.create_table("state_diffs")?,
        deployed_contracts: db_writer.create_table("contracts")?,
        declared_classes: db_writer.create_table("contract_classes")?,
        contract_storage: db_writer.create_table("contract_storage")?,
    });
    let reader = StorageReader { db_reader, tables: tables.clone() };
    let writer = StorageWriter { db_writer, tables };
    Ok((reader, writer))
}
