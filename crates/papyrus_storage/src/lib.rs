pub mod body;
pub mod compression_utils;
pub mod db;
pub mod header;
pub mod ommer;
mod serializers;
pub mod state;
mod version;

#[cfg(any(feature = "testing", test))]
#[path = "test_utils.rs"]
pub mod test_utils;

use std::collections::HashMap;
use std::sync::Arc;

use db::DbTableStats;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{ContractClass, StorageKey};
use starknet_api::transaction::{
    EventContent, EventIndexInTransactionOutput, Transaction, TransactionHash,
    TransactionOffsetInBlock,
};
use tracing::debug;
use version::{StorageVersionError, Version};

use crate::body::events::ThinTransactionOutput;
use crate::db::{
    open_env, DbConfig, DbError, DbReader, DbTransaction, DbWriter, TableHandle, TableIdentifier,
    TransactionKind, RO, RW,
};
use crate::state::data::{
    IndexedContractClass, IndexedDeployedContract, IndexedDeprecatedContractClass, ThinStateDiff,
};
use crate::version::{VersionStorageReader, VersionStorageWriter};

const STORAGE_VERSION_UINT: u32 = 0;
pub const STORAGE_VERSION: Version = Version(STORAGE_VERSION_UINT);

pub fn open_storage(db_config: DbConfig) -> StorageResult<(StorageReader, StorageWriter)> {
    let (db_reader, mut db_writer) = open_env(db_config)?;
    let tables = Arc::new(Tables {
        block_hash_to_number: db_writer.create_table("block_hash_to_number")?,
        contract_storage: db_writer.create_table("contract_storage")?,
        declared_classes: db_writer.create_table("declared_classes")?,
        deprecated_declared_classes: db_writer.create_table("deprecated_declared_classes")?,
        deployed_contracts: db_writer.create_table("deployed_contracts")?,
        events: db_writer.create_table("events")?,
        headers: db_writer.create_table("headers")?,
        markers: db_writer.create_table("markers")?,
        nonces: db_writer.create_table("nonces")?,
        ommer_contract_storage: db_writer.create_table("ommer_contract_storage")?,
        ommer_declared_classes: db_writer.create_table("ommer_declared_classes")?,
        ommer_deployed_contracts: db_writer.create_table("ommer_deployed_contracts")?,
        ommer_events: db_writer.create_table("ommer_events")?,
        ommer_headers: db_writer.create_table("ommer_headers")?,
        ommer_nonces: db_writer.create_table("ommer_nonces")?,
        ommer_state_diffs: db_writer.create_table("ommer_state_diffs")?,
        ommer_transaction_outputs: db_writer.create_table("ommer_transaction_outputs")?,
        ommer_transactions: db_writer.create_table("ommer_transactions")?,
        replaced_classes: db_writer.create_table("replaced_classes")?,
        state_diffs: db_writer.create_table("state_diffs")?,
        transaction_hash_to_idx: db_writer.create_table("transaction_hash_to_idx")?,
        transaction_outputs: db_writer.create_table("transaction_outputs")?,
        transactions: db_writer.create_table("transactions")?,
        storage_version: db_writer.create_table("storage_version")?,
    });
    let reader = StorageReader { db_reader, tables: tables.clone() };
    let writer = StorageWriter { db_writer, tables };

    let writer = set_initial_version_if_needed(writer)?;
    verify_storage_version(reader.clone())?;
    Ok((reader, writer))
}

// In case storage version does not exist, set it to the crate version.
// Expected to happen once - when the node is launched for the first time.
fn set_initial_version_if_needed(mut writer: StorageWriter) -> StorageResult<StorageWriter> {
    let current_storage_version = writer.begin_rw_txn()?.get_version()?;
    if current_storage_version.is_none() {
        writer.begin_rw_txn()?.set_version(&STORAGE_VERSION)?.commit()?;
    };
    Ok(writer)
}

// Assumes the storage has a version.
fn verify_storage_version(reader: StorageReader) -> StorageResult<()> {
    debug!("Storage crate version = {STORAGE_VERSION:}.");
    let current_storage_version =
        reader.begin_ro_txn()?.get_version()?.expect("Storage should have a version");
    debug!("Current storage version = {current_storage_version:}.");

    if STORAGE_VERSION != current_storage_version {
        return Err(StorageError::StorageVersionInconcistency(
            StorageVersionError::InconsistentStorageVersion {
                crate_version: STORAGE_VERSION,
                storage_version: current_storage_version,
            },
        ));
    }
    Ok(())
}

#[derive(Clone)]
pub struct StorageReader {
    db_reader: DbReader,
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

pub struct StorageWriter {
    db_writer: DbWriter,
    tables: Arc<Tables>,
}

impl StorageWriter {
    pub fn begin_rw_txn(&mut self) -> StorageResult<StorageTxn<'_, RW>> {
        Ok(StorageTxn { txn: self.db_writer.begin_rw_txn()?, tables: self.tables.clone() })
    }
}

pub struct StorageTxn<'env, Mode: TransactionKind> {
    txn: DbTransaction<'env, Mode>,
    tables: Arc<Tables>,
}

impl<'env> StorageTxn<'env, RW> {
    pub fn commit(self) -> StorageResult<()> {
        Ok(self.txn.commit()?)
    }
}

pub fn table_names() -> &'static [&'static str] {
    Tables::field_names()
}

struct_field_names! {
    struct Tables {
        block_hash_to_number: TableIdentifier<BlockHash, BlockNumber>,
        contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockNumber), StarkFelt>,
        declared_classes: TableIdentifier<ClassHash, IndexedContractClass>,
        deprecated_declared_classes: TableIdentifier<ClassHash, IndexedDeprecatedContractClass>,
        deployed_contracts: TableIdentifier<ContractAddress, IndexedDeployedContract>,
        events: TableIdentifier<(ContractAddress, EventIndex), EventContent>,
        headers: TableIdentifier<BlockNumber, BlockHeader>,
        markers: TableIdentifier<MarkerKind, BlockNumber>,
        nonces: TableIdentifier<(ContractAddress, BlockNumber), Nonce>,
        ommer_contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockHash), StarkFelt>,
        //TODO(yair): Consider whether an ommer_deprecated_declared_classes is needed.
        ommer_declared_classes: TableIdentifier<(BlockHash, ClassHash), ContractClass>,
        ommer_deployed_contracts: TableIdentifier<(ContractAddress, BlockHash), ClassHash>,
        ommer_events: TableIdentifier<(ContractAddress, OmmerEventKey), EventContent>,
        ommer_headers: TableIdentifier<BlockHash, BlockHeader>,
        ommer_nonces: TableIdentifier<(ContractAddress, BlockHash), Nonce>,
        ommer_state_diffs: TableIdentifier<BlockHash, ThinStateDiff>,
        ommer_transaction_outputs: TableIdentifier<OmmerTransactionKey, ThinTransactionOutput>,
        ommer_transactions: TableIdentifier<OmmerTransactionKey, Transaction>,
        replaced_classes: TableIdentifier<(ContractAddress, BlockNumber),ClassHash>,
        state_diffs: TableIdentifier<BlockNumber, ThinStateDiff>,
        transaction_hash_to_idx: TableIdentifier<TransactionHash, TransactionIndex>,
        transaction_outputs: TableIdentifier<TransactionIndex, ThinTransactionOutput>,
        transactions: TableIdentifier<TransactionIndex, Transaction>,
        storage_version: TableIdentifier<String, Version>
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
use struct_field_names;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error(transparent)]
    InnerError(#[from] DbError),
    #[error("Marker mismatch (expected {expected}, found {found}).")]
    MarkerMismatch { expected: BlockNumber, found: BlockNumber },
    #[error("Block hash {block_hash} already exists, when adding block number {block_number}.")]
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
         {block_number}."
    )]
    NonceReWrite { nonce: Nonce, block_number: BlockNumber, contract_address: ContractAddress },
    #[error(
        "Event with index {event_index:?} emitted from contract address {from_address:?} was not \
         found."
    )]
    EventNotFound { event_index: EventIndex, from_address: ContractAddress },
    #[error("DB in inconsistent state: {msg:?}.")]
    DBInconsistency { msg: String },
    #[error("Header of block with hash {block_hash} already exists in ommer table.")]
    OmmerHeaderAlreadyExists { block_hash: BlockHash },
    #[error("Ommer transaction key {tx_key:?} already exists.")]
    OmmerTransactionKeyAlreadyExists { tx_key: OmmerTransactionKey },
    #[error("Ommer transaction output key {tx_key:?} already exists.")]
    OmmerTransactionOutputKeyAlreadyExists { tx_key: OmmerTransactionKey },
    #[error(
        "Ommer event {event_key:?} emitted from contract address {contract_address:?} already \
         exists."
    )]
    OmmerEventAlreadyExists { contract_address: ContractAddress, event_key: OmmerEventKey },
    #[error("Ommer state diff of block {block_hash} already exists.")]
    OmmerStateDiffAlreadyExists { block_hash: BlockHash },
    #[error("Ommer class {class_hash:?} of block {block_hash} already exists.")]
    OmmerClassAlreadyExists { block_hash: BlockHash, class_hash: ClassHash },
    #[error("Ommer deployed contract {contract_address:?} of block {block_hash} already exists.")]
    OmmerDeployedContractAlreadyExists { block_hash: BlockHash, contract_address: ContractAddress },
    #[error(
        "Ommer storage key {key:?} of contract {contract_address:?} of block {block_hash} already \
         exists."
    )]
    OmmerStorageKeyAlreadyExists {
        block_hash: BlockHash,
        contract_address: ContractAddress,
        key: StorageKey,
    },
    #[error("Ommer nonce of contract {contract_address:?} of block {block_hash} already exists.")]
    OmmerNonceAlreadyExists { block_hash: BlockHash, contract_address: ContractAddress },
    #[error(transparent)]
    StorageVersionInconcistency(#[from] StorageVersionError),
}

pub type StorageResult<V> = std::result::Result<V, StorageError>;

#[derive(Serialize, Deserialize, Clone)]
pub struct StorageConfig {
    pub db_config: DbConfig,
}

/// A mapping from a table name in the database to its statistics.
#[derive(Serialize, Deserialize, Debug)]
pub struct DbTablesStats {
    pub stats: HashMap<String, DbTableStats>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum MarkerKind {
    Header,
    Body,
    State,
}

pub type MarkersTable<'env> = TableHandle<'env, MarkerKind, BlockNumber>;

// TODO(yair): move the key structs from the main lib file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct OmmerTransactionKey(pub BlockHash, pub TransactionOffsetInBlock);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct OmmerEventKey(pub OmmerTransactionKey, pub EventIndexInTransactionOutput);
