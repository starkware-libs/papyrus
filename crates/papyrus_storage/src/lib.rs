// config compiler to support no_coverage feature when running coverage in nightly mode within this
// crate
#![cfg_attr(coverage_nightly, feature(no_coverage))]
#![warn(missing_docs)]

//! A storage implementation for a [`Starknet`] node.
//!
//! This crate provides a writing and reading interface for various Starknet data structures to a
//! database. Enables at most one writing operation and multiple reading operations concurrently.
//! The underlying storage is implemented using the [`libmdbx`] crate.
//!
//! # Disclaimer
//! This crate is still under development and is not keeping backwards compatibility with previous
//! versions. Breaking changes are expected to happen in the near future.
//!
//! # Quick Start
//! To use this crate, open a storage by calling [`open_storage`] to get a [`StorageWriter`] and a
//! [`StorageReader`] and use them to create [`StorageTxn`] instances. The actual
//! functionality is implemented on the transaction in multiple traits.
//!
//! ```
//! use papyrus_storage::open_storage;
//! use papyrus_storage::db::DbConfig;
//! use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter, StarknetVersion};    // Import the header API.
//! use starknet_api::block::{BlockHeader, BlockNumber};
//! use starknet_api::core::ChainId;
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! let db_config = DbConfig {
//!     path_prefix: dir,
//!     chain_id: ChainId("SN_MAIN".to_owned()),
//!     min_size: 1 << 20,    // 1MB
//!     max_size: 1 << 35,    // 32GB
//!     growth_step: 1 << 26, // 64MB
//! };
//! let (reader, mut writer) = open_storage(db_config).unwrap();
//! writer
//!     .begin_rw_txn()?                                            // Start a RW transaction.
//!     .append_header(BlockNumber(0), &BlockHeader::default())?    // Append a header.
//!     .commit()?;                                                 // Commit the changes.
//!
//! let header = reader.begin_ro_txn()?.get_block_header(BlockNumber(0))?;  // Read the header.
//! assert_eq!(header, Some(BlockHeader::default()));
//! # Ok::<(), papyrus_storage::StorageError>(())
//! ```
//!
//! [`Starknet`]: https://starknet.io/
//! [`libmdbx`]: https://docs.rs/libmdbx/latest/libmdbx/

pub mod base_layer;
pub mod body;
pub mod compiled_class;
pub mod mmap_objects_db;
// TODO(yair): Make the compression_utils module pub(crate) or extract it from the crate.
#[doc(hidden)]
pub mod compression_utils;
pub mod db;
pub mod header;
// TODO(yair): Once decided whether to keep the ommer module, write its documentation or delete it.
#[doc(hidden)]
pub mod ommer;
mod serializers;
pub mod state;
mod version;

#[cfg(test)]
mod test_instances;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use body::events::EventIndex;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use db::DbTableStats;
use mmap_objects_db::{LargeFile, LocationInFile};
use ommer::{OmmerEventKey, OmmerTransactionKey};
use papyrus_config::dumping::{append_sub_config_name, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{ContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::{EventContent, Transaction, TransactionHash};
use tracing::debug;
use validator::Validate;
use version::{StorageVersionError, Version};

use crate::body::events::ThinTransactionOutput;
use crate::body::TransactionIndex;
use crate::db::{
    open_env,
    DbConfig,
    DbError,
    DbReader,
    DbTransaction,
    DbWriter,
    TableHandle,
    TableIdentifier,
    TransactionKind,
    RO,
    RW,
};
use crate::header::StarknetVersion;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::version::{VersionStorageReader, VersionStorageWriter};

/// The current version of the storage code.
/// Whenever a breaking change is introduced, the version is incremented and a storage
/// migration is required for existing storages.
pub const STORAGE_VERSION: Version = Version(4);

/// Storage that is backed up by mmap files directly, instead of using the database.
#[derive(Clone)]
pub struct StorageFiles {
    class: Arc<Mutex<LargeFile<ContractClass>>>,
    thin_state_diff: Arc<Mutex<LargeFile<ThinStateDiff>>>,
    casm: Arc<Mutex<LargeFile<CasmContractClass>>>,
}

/// Opens a storage and returns a [`StorageReader`] and a [`StorageWriter`].
pub fn open_storage(db_config: DbConfig) -> StorageResult<(StorageReader, StorageWriter)> {
    let (db_reader, mut db_writer) = open_env(&db_config)?;
    let storage_files = open_storage_files(&db_config);
    // let class_storage = Arc::new(Mutex::new(mmap_objects_db::open_mmaped_file(
    //     db_config.path().join("large_object_db"),
    // )));
    let tables = Arc::new(Tables {
        block_hash_to_number: db_writer.create_table("block_hash_to_number")?,
        casms: db_writer.create_table("casms")?,
        contract_storage: db_writer.create_table("contract_storage")?,
        declared_classes: db_writer.create_table("declared_classes")?,
        declared_classes_block: db_writer.create_table("declared_classes_block")?,
        deprecated_declared_classes: db_writer.create_table("deprecated_declared_classes")?,
        deployed_contracts: db_writer.create_table("deployed_contracts")?,
        events: db_writer.create_table("events")?,
        headers: db_writer.create_table("headers")?,
        markers: db_writer.create_table("markers")?,
        nonces: db_writer.create_table("nonces")?,
        offsets: db_writer.create_table("offsets")?,
        ommer_contract_storage: db_writer.create_table("ommer_contract_storage")?,
        ommer_declared_classes: db_writer.create_table("ommer_declared_classes")?,
        ommer_deployed_contracts: db_writer.create_table("ommer_deployed_contracts")?,
        ommer_events: db_writer.create_table("ommer_events")?,
        ommer_headers: db_writer.create_table("ommer_headers")?,
        ommer_nonces: db_writer.create_table("ommer_nonces")?,
        ommer_state_diffs: db_writer.create_table("ommer_state_diffs")?,
        ommer_transaction_outputs: db_writer.create_table("ommer_transaction_outputs")?,
        ommer_transactions: db_writer.create_table("ommer_transactions")?,
        state_diffs: db_writer.create_table("state_diffs")?,
        transaction_hash_to_idx: db_writer.create_table("transaction_hash_to_idx")?,
        transaction_idx_to_hash: db_writer.create_table("transaction_idx_to_hash")?,
        transaction_outputs: db_writer.create_table("transaction_outputs")?,
        transactions: db_writer.create_table("transactions")?,
        starknet_version: db_writer.create_table("starknet_version")?,
        storage_version: db_writer.create_table("storage_version")?,
    });
    let reader =
        StorageReader { db_reader, tables: tables.clone(), storage_files: storage_files.clone() };
    let writer = StorageWriter { db_writer, tables, storage_files };

    let writer = set_initial_version_if_needed(writer)?;
    verify_storage_version(reader.clone())?;
    Ok((reader, writer))
}

fn open_storage_files(db_config: &DbConfig) -> StorageFiles {
    StorageFiles {
        class: Arc::new(Mutex::new(mmap_objects_db::open_mmaped_file(
            db_config.path().join("class"),
        ))),
        thin_state_diff: Arc::new(Mutex::new(mmap_objects_db::open_mmaped_file(
            db_config.path().join("thin_state_diff"),
        ))),
        casm: Arc::new(Mutex::new(mmap_objects_db::open_mmaped_file(
            db_config.path().join("casm"),
        ))),
    }
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

/// A struct for starting RO transactions ([`StorageTxn`]) to the storage.
#[derive(Clone)]
pub struct StorageReader {
    db_reader: DbReader,
    tables: Arc<Tables>,
    // class_storage: Arc<Mutex<LargeFile>>,
    storage_files: StorageFiles,
}

impl StorageReader {
    /// Takes a snapshot of the current state of the storage and returns a [`StorageTxn`] for
    /// reading data from the storage.
    pub fn begin_ro_txn(&self) -> StorageResult<StorageTxn<'_, RO>> {
        Ok(StorageTxn {
            txn: self.db_reader.begin_ro_txn()?,
            tables: self.tables.clone(),
            // class_storage: self.class_storage.clone(),
            storage_files: self.storage_files.clone(),
        })
    }

    /// Returns metadata about the tables in the storage.
    pub fn db_tables_stats(&self) -> StorageResult<DbTablesStats> {
        let mut stats = HashMap::new();
        for name in Tables::field_names() {
            stats.insert(name.to_string(), self.db_reader.get_table_stats(name)?);
        }
        Ok(DbTablesStats { stats })
    }
}

/// A struct for starting RW transactions ([`StorageTxn`]) to the storage.
/// There is a single non clonable writer instance, to make sure there is only one write transaction
/// at any given moment.
pub struct StorageWriter {
    db_writer: DbWriter,
    tables: Arc<Tables>,
    // class_storage: Arc<Mutex<LargeFile>>,
    storage_files: StorageFiles,
}

impl StorageWriter {
    /// Takes a snapshot of the current state of the storage and returns a [`StorageTxn`] for
    /// reading and modifying data in the storage.
    pub fn begin_rw_txn(&mut self) -> StorageResult<StorageTxn<'_, RW>> {
        Ok(StorageTxn {
            txn: self.db_writer.begin_rw_txn()?,
            tables: self.tables.clone(),
            // class_storage: self.class_storage.clone(),
            storage_files: self.storage_files.clone(),
        })
    }
}

/// A struct for interacting with the storage.
/// The actually functionality is implemented on the transaction in multiple traits.
pub struct StorageTxn<'env, Mode: TransactionKind> {
    txn: DbTransaction<'env, Mode>,
    tables: Arc<Tables>,
    // class_storage: Arc<Mutex<LargeFile>>,
    storage_files: StorageFiles,
}

impl<'env> StorageTxn<'env, RW> {
    /// Commits the changes made in the transaction to the storage.
    pub fn commit(self) -> StorageResult<()> {
        Ok(self.txn.commit()?)
    }
}

/// Returns the names of the tables in the storage.
pub fn table_names() -> &'static [&'static str] {
    Tables::field_names()
}

struct_field_names! {
    struct Tables {
        block_hash_to_number: TableIdentifier<BlockHash, BlockNumber>,
        casms: TableIdentifier<ClassHash, LocationInFile>,
        contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockNumber), StarkFelt>,
        declared_classes: TableIdentifier<ClassHash, LocationInFile>,
        declared_classes_block: TableIdentifier<ClassHash, BlockNumber>,
        deprecated_declared_classes: TableIdentifier<ClassHash, IndexedDeprecatedContractClass>,
        deployed_contracts: TableIdentifier<(ContractAddress, BlockNumber), ClassHash>,
        events: TableIdentifier<(ContractAddress, EventIndex), EventContent>,
        headers: TableIdentifier<BlockNumber, BlockHeader>,
        markers: TableIdentifier<MarkerKind, BlockNumber>,
        nonces: TableIdentifier<(ContractAddress, BlockNumber), Nonce>,
        offsets: TableIdentifier<OffsetKind, usize>,
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
        state_diffs: TableIdentifier<BlockNumber, LocationInFile>,
        transaction_hash_to_idx: TableIdentifier<TransactionHash, TransactionIndex>,
        transaction_idx_to_hash: TableIdentifier<TransactionIndex, TransactionHash>,
        transaction_outputs: TableIdentifier<TransactionIndex, ThinTransactionOutput>,
        transactions: TableIdentifier<TransactionIndex, Transaction>,
        starknet_version: TableIdentifier<BlockNumber, StarknetVersion>,
        storage_version: TableIdentifier<String, Version>
    }
}

macro_rules! struct_field_names {
    (struct $name:ident { $($fname:ident : $ftype:ty),* }) => {
        pub(crate) struct $name {
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

/// Error type for the storage crate.
#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    /// Errors related to the underlying database.
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
    #[error("Compiled class of {class_hash:?} already exists.")]
    CompiledClassReWrite { class_hash: ClassHash },
}

/// A type alias that maps to std::result::Result<T, StorageError>.
pub type StorageResult<V> = std::result::Result<V, StorageError>;

/// A struct for the configuration of the storage.
#[allow(missing_docs)]
#[derive(Serialize, Debug, Deserialize, Clone, Default, PartialEq, Validate)]
pub struct StorageConfig {
    #[validate]
    pub db_config: DbConfig,
}

impl SerializeConfig for StorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        append_sub_config_name(self.db_config.dump(), "db_config")
    }
}

/// A struct for the statistics of the tables in the database.
#[derive(Serialize, Deserialize, Debug)]
pub struct DbTablesStats {
    /// A mapping from a table name in the database to its statistics.
    pub stats: HashMap<String, DbTableStats>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
// A marker is the first block number for which the corresponding data doesn't exist yet.
// Invariants:
// - CompiledClass <= State <= Header
// - Body <= Header
// - BaseLayerBlock <= Header
pub(crate) enum MarkerKind {
    Header,
    Body,
    State,
    CompiledClass,
    BaseLayerBlock,
}

pub(crate) type MarkersTable<'env> = TableHandle<'env, MarkerKind, BlockNumber>;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum OffsetKind {
    Class,
    ThinStateDiff,
    Casm,
}
