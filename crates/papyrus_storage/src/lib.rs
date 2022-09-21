mod body;
mod db;
mod header;
mod ommer;
mod serializers;
mod state;

#[cfg(any(feature = "testing", test))]
#[path = "test_utils.rs"]
pub mod test_utils;

use std::collections::HashMap;
use std::sync::Arc;

use db::DbTableStats;
use serde::{Deserialize, Serialize};
use starknet_api::{
    Block, BlockHash, BlockHeader, BlockNumber, ClassHash, ContractAddress, DeclaredContract,
    Nonce, StarkFelt, StateDiff, StorageKey, Transaction, TransactionHash,
    TransactionOffsetInBlock, TransactionOutput,
};
use state::{IndexedDeclaredContract, IndexedDeployedContract};

pub use self::body::{BodyStorageReader, BodyStorageWriter};
pub use self::db::TransactionKind;
use self::db::{
    open_env, DbConfig, DbError, DbReader, DbTransaction, DbWriter, TableHandle, TableIdentifier,
    RO, RW,
};
pub use self::header::{HeaderStorageReader, HeaderStorageWriter};
pub use self::ommer::OmmerStorageWriter;
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
         {transaction_index:?}."
    )]
    TransactionHashAlreadyExists { tx_hash: TransactionHash, transaction_index: TransactionIndex },
    #[error("State diff redployed to an existing contract address {address:?}.")]
    ContractAlreadyExists { address: ContractAddress },
    #[error(
        "State diff redeclared a different class to an existing contract hash {class_hash:?}."
    )]
    ClassAlreadyExists { class_hash: ClassHash },
    #[error(
        "State diff redefined a nonce {nonce:?} for contract {contract_address:?} at block \
         {block_number:?}."
    )]
    NonceReWrite { nonce: Nonce, block_number: BlockNumber, contract_address: ContractAddress },
    #[error(
        "Cannot revert block {revert_block_number:?}, current marker is {block_number_marker:?}."
    )]
    InvalidRevert { revert_block_number: BlockNumber, block_number_marker: BlockNumber },
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}
pub type StorageResult<V> = std::result::Result<V, StorageError>;

/// A mapping from a table name in the database to its statistics.
#[derive(Serialize, Deserialize, Debug)]
pub struct DbTablesStats {
    pub stats: HashMap<String, DbTableStats>,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum MarkerKind {
    Header,
    Body,
    State,
}
pub type MarkersTable<'env> = TableHandle<'env, MarkerKind, BlockNumber>;

// Keeping serialization of (StateDiff, Vec<DeclaredContract>) since the storage serde doesn't
// handle complex types.
#[derive(Serialize, Deserialize)]
struct SerializedOmmerStateDiff(String);
impl SerializedOmmerStateDiff {
    pub fn try_from(pair: &(StateDiff, Vec<DeclaredContract>)) -> StorageResult<Self> {
        let serialized = serde_json::to_string(pair).map_err(StorageError::SerdeError)?;
        Ok(Self(serialized))
    }

    #[cfg(test)]
    pub fn try_into(self) -> StorageResult<(StateDiff, Vec<DeclaredContract>)> {
        let value: (StateDiff, Vec<DeclaredContract>) =
            serde_json::from_str(self.0.as_str()).map_err(StorageError::SerdeError)?;
        Ok(value)
    }
}

macro_rules! struct_field_names {
    (struct $name:ident { $($fname:ident : $ftype:ty),* }) => {
        pub struct $name {
            $($fname : $ftype),*
        }

        impl $name {
            fn field_names() -> &'static [&'static str] {
                static NAMES: &'static [&'static str] = &[$(stringify!($fname)),*];
                NAMES
            }
        }
    }
}

struct_field_names! {
    struct Tables {
        block_hash_to_number: TableIdentifier<BlockHash, BlockNumber>,
        contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockNumber), StarkFelt>,
        declared_classes: TableIdentifier<ClassHash, IndexedDeclaredContract>,
        deployed_contracts: TableIdentifier<ContractAddress, IndexedDeployedContract>,
        headers: TableIdentifier<BlockNumber, BlockHeader>,
        markers: TableIdentifier<MarkerKind, BlockNumber>,
        nonces: TableIdentifier<(ContractAddress, BlockNumber), Nonce>,
        ommer_blocks: TableIdentifier<BlockHash, Block>,
        ommer_state_diffs: TableIdentifier<BlockHash, SerializedOmmerStateDiff>,
        state_diffs: TableIdentifier<BlockNumber, ThinStateDiff>,
        transaction_hash_to_idx: TableIdentifier<TransactionHash, TransactionIndex>,
        transaction_outputs: TableIdentifier<TransactionIndex, TransactionOutput>,
        transactions: TableIdentifier<TransactionIndex, Transaction>
    }
}

pub fn table_names() -> &'static [&'static str] {
    Tables::field_names()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);

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
    pub fn db_tables_stats(&self) -> StorageResult<DbTablesStats> {
        let mut stats = HashMap::new();
        for name in Tables::field_names() {
            stats.insert(name.to_string(), self.db_reader.get_table_stats(name)?);
        }
        Ok(DbTablesStats { stats })
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
        block_hash_to_number: db_writer.create_table("block_hash_to_number")?,
        contract_storage: db_writer.create_table("contract_storage")?,
        declared_classes: db_writer.create_table("declared_classes")?,
        deployed_contracts: db_writer.create_table("deployed_contracts")?,
        headers: db_writer.create_table("headers")?,
        ommer_blocks: db_writer.create_table("ommer_blocks")?,
        ommer_state_diffs: db_writer.create_table("ommer_state_diffs")?,
        markers: db_writer.create_table("markers")?,
        nonces: db_writer.create_table("nonces")?,
        state_diffs: db_writer.create_table("state_diffs")?,
        transaction_hash_to_idx: db_writer.create_table("transaction_hash_to_idx")?,
        transaction_outputs: db_writer.create_table("transaction_outputs")?,
        transactions: db_writer.create_table("transactions")?,
    });
    let reader = StorageReader { db_reader, tables: tables.clone() };
    let writer = StorageWriter { db_writer, tables };
    Ok((reader, writer))
}
