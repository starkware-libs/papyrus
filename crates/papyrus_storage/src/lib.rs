// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
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
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
//! use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter, StarknetVersion};    // Import the header API.
//! use starknet_api::block::{BlockHeader, BlockNumber};
//! use starknet_api::core::ChainId;
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! let db_config = DbConfig {
//!     path_prefix: dir,
//!     chain_id: ChainId("SN_MAIN".to_owned()),
//!     enforce_file_exists: false,
//!     min_size: 1 << 20,    // 1MB
//!     max_size: 1 << 35,    // 32GB
//!     growth_step: 1 << 26, // 64MB
//! };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let (reader, mut writer) = open_storage(storage_config)?;
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
pub mod utils;
// TODO(yair): Make the compression_utils module pub(crate) or extract it from the crate.
#[doc(hidden)]
pub mod compression_utils;
pub mod db;
pub mod header;
pub mod mmap_file;
mod serializers;
pub mod state;
mod version;

#[cfg(test)]
mod test_instances;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

use body::events::EventIndex;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use db::db_stats::{DbTableStats, DbWholeStats};
use db::serialization::{Key, NoVersionValueWrapper, ValueSerde};
use db::table_types::Table;
use mmap_file::{
    open_file,
    FileHandler,
    LocationInFile,
    MMapFileError,
    MmapFileConfig,
    Reader,
    Writer,
};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::{EventContent, Transaction, TransactionHash};
use starknet_types_core::felt::Felt;
use tracing::{debug, warn};
use validator::Validate;
use version::{StorageVersionError, Version};

use crate::body::events::ThinTransactionOutput;
use crate::body::TransactionIndex;
use crate::db::table_types::SimpleTable;
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
pub use crate::utils::update_storage_metrics;
use crate::version::{VersionStorageReader, VersionStorageWriter};

/// The current version of the storage state code.
/// Whenever a breaking change is introduced, the version is incremented and a storage
/// migration is required for existing storages.
pub const STORAGE_VERSION_STATE: Version = Version(11);
/// The current version of the storage blocks code.
/// Whenever a breaking change is introduced, the version is incremented and a storage
/// migration is required for existing storages.
/// This version is only checked for storages that store transactions (StorageScope::FullArchive).
pub const STORAGE_VERSION_BLOCKS: Version = Version(11);

/// Opens a storage and returns a [`StorageReader`] and a [`StorageWriter`].
pub fn open_storage(
    storage_config: StorageConfig,
) -> StorageResult<(StorageReader, StorageWriter)> {
    let (db_reader, mut db_writer) = open_env(&storage_config.db_config)?;
    let tables = Arc::new(Tables {
        block_hash_to_number: db_writer.create_simple_table("block_hash_to_number")?,
        block_signatures: db_writer.create_simple_table("block_signatures")?,
        casms: db_writer.create_simple_table("casms")?,
        contract_storage: db_writer.create_simple_table("contract_storage")?,
        declared_classes: db_writer.create_simple_table("declared_classes")?,
        declared_classes_block: db_writer.create_simple_table("declared_classes_block")?,
        deprecated_declared_classes: db_writer
            .create_simple_table("deprecated_declared_classes")?,
        deployed_contracts: db_writer.create_simple_table("deployed_contracts")?,
        events: db_writer.create_simple_table("events")?,
        headers: db_writer.create_simple_table("headers")?,
        markers: db_writer.create_simple_table("markers")?,
        nonces: db_writer.create_simple_table("nonces")?,
        file_offsets: db_writer.create_simple_table("file_offsets")?,
        state_diffs: db_writer.create_simple_table("state_diffs")?,
        transaction_hash_to_idx: db_writer.create_simple_table("transaction_hash_to_idx")?,
        transaction_idx_to_hash: db_writer.create_simple_table("transaction_idx_to_hash")?,
        transaction_outputs: db_writer.create_simple_table("transaction_outputs")?,
        transactions: db_writer.create_simple_table("transactions")?,

        // Version tables
        starknet_version: db_writer.create_simple_table("starknet_version")?,
        storage_version: db_writer.create_simple_table("storage_version")?,
    });
    let (file_writers, file_readers) = open_storage_files(
        &storage_config.db_config,
        storage_config.mmap_file_config,
        db_reader.clone(),
        &tables.file_offsets,
    )?;

    let reader = StorageReader {
        db_reader,
        tables: tables.clone(),
        scope: storage_config.scope,
        file_readers,
    };
    let writer = StorageWriter { db_writer, tables, scope: storage_config.scope, file_writers };

    let writer = set_version_if_needed(reader.clone(), writer)?;
    verify_storage_version(reader.clone())?;
    Ok((reader, writer))
}

// In case storage version does not exist, set it to the crate version.
// Expected to happen once - when the node is launched for the first time.
// If the storage scope has changed, update accordingly.
fn set_version_if_needed(
    reader: StorageReader,
    mut writer: StorageWriter,
) -> StorageResult<StorageWriter> {
    let existing_storage_version = get_storage_version(reader)?;
    debug!("Existing storage state: {:?}", existing_storage_version);
    match existing_storage_version {
        None => {
            // Initialize the storage version.
            writer.begin_rw_txn()?.set_state_version(&STORAGE_VERSION_STATE)?.commit()?;
            // If in full-archive mode, also set the block version.
            if writer.scope == StorageScope::FullArchive {
                writer.begin_rw_txn()?.set_blocks_version(&STORAGE_VERSION_BLOCKS)?.commit()?;
            }
            debug!(
                "Storage was initialized with state_version: {:?}, scope: {:?}, blocks_version: \
                 {:?}",
                STORAGE_VERSION_STATE, writer.scope, STORAGE_VERSION_BLOCKS
            );
        }
        Some(StorageVersion::FullArchive(FullArchiveVersion {
            state_version: _,
            blocks_version: _,
        })) => {
            // TODO(yael): consider optimizing by deleting the block's data if the scope has changed
            // to StateOnly
            if writer.scope == StorageScope::StateOnly {
                // Deletion of the block's version is required here. It ensures that the node knows
                // that the storage operates in StateOnly mode and prevents the operator from
                // running it in FullArchive mode again.
                writer.begin_rw_txn()?.delete_blocks_version()?.commit()?;
            }
        }
        Some(StorageVersion::StateOnly(StateOnlyVersion { state_version: _ })) => {
            // The storage cannot change from state-only to full-archive mode.
            if writer.scope == StorageScope::FullArchive {
                return Err(StorageError::StorageVersionInconsistency(
                    StorageVersionError::InconsistentStorageScope,
                ));
            }
        }
    }
    Ok(writer)
}

#[derive(Debug)]
struct FullArchiveVersion {
    state_version: Version,
    blocks_version: Version,
}

#[derive(Debug)]
struct StateOnlyVersion {
    state_version: Version,
}

#[derive(Debug)]
enum StorageVersion {
    FullArchive(FullArchiveVersion),
    StateOnly(StateOnlyVersion),
}

fn get_storage_version(reader: StorageReader) -> StorageResult<Option<StorageVersion>> {
    let current_storage_version_state = reader.begin_ro_txn()?.get_state_version()?;
    let current_storage_version_blocks = reader.begin_ro_txn()?.get_blocks_version()?;
    let Some(current_storage_version_state) = current_storage_version_state else {
        return Ok(None);
    };
    match current_storage_version_blocks {
        Some(current_storage_version_blocks) => {
            Ok(Some(StorageVersion::FullArchive(FullArchiveVersion {
                state_version: current_storage_version_state,
                blocks_version: current_storage_version_blocks,
            })))
        }
        None => Ok(Some(StorageVersion::StateOnly(StateOnlyVersion {
            state_version: current_storage_version_state,
        }))),
    }
}

// Assumes the storage has a version.
fn verify_storage_version(reader: StorageReader) -> StorageResult<()> {
    let existing_storage_version = get_storage_version(reader)?;
    debug!(
        "Crate storage version: State = {STORAGE_VERSION_STATE:} Blocks = \
         {STORAGE_VERSION_BLOCKS:}. Existing storage state: {existing_storage_version:?} "
    );

    match existing_storage_version {
        None => panic!("Storage should be initialized."),
        Some(StorageVersion::FullArchive(FullArchiveVersion {
            state_version: existing_state_version,
            blocks_version: _,
        })) if STORAGE_VERSION_STATE != existing_state_version => {
            Err(StorageError::StorageVersionInconsistency(
                StorageVersionError::InconsistentStorageVersion {
                    crate_version: STORAGE_VERSION_STATE,
                    storage_version: existing_state_version,
                },
            ))
        }

        Some(StorageVersion::FullArchive(FullArchiveVersion {
            state_version: _,
            blocks_version: existing_blocks_version,
        })) if STORAGE_VERSION_BLOCKS != existing_blocks_version => {
            Err(StorageError::StorageVersionInconsistency(
                StorageVersionError::InconsistentStorageVersion {
                    crate_version: STORAGE_VERSION_BLOCKS,
                    storage_version: existing_blocks_version,
                },
            ))
        }

        Some(StorageVersion::StateOnly(StateOnlyVersion {
            state_version: existing_state_version,
        })) if STORAGE_VERSION_STATE != existing_state_version => {
            Err(StorageError::StorageVersionInconsistency(
                StorageVersionError::InconsistentStorageVersion {
                    crate_version: STORAGE_VERSION_STATE,
                    storage_version: existing_state_version,
                },
            ))
        }
        Some(_) => Ok(()),
    }
}

/// The categories of data to save in the storage.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum StorageScope {
    /// Stores all types of data.
    #[default]
    FullArchive,
    /// Stores the data describing the current state. In this mode the transaction, events and
    /// state-diffs are not stored.
    StateOnly,
}

/// A struct for starting RO transactions ([`StorageTxn`]) to the storage.
#[derive(Clone)]
pub struct StorageReader {
    db_reader: DbReader,
    file_readers: FileHandlers<RO>,
    tables: Arc<Tables>,
    scope: StorageScope,
}

impl StorageReader {
    /// Takes a snapshot of the current state of the storage and returns a [`StorageTxn`] for
    /// reading data from the storage.
    pub fn begin_ro_txn(&self) -> StorageResult<StorageTxn<'_, RO>> {
        Ok(StorageTxn {
            txn: self.db_reader.begin_ro_txn()?,
            file_handlers: self.file_readers.clone(),
            tables: self.tables.clone(),
            scope: self.scope,
        })
    }

    /// Returns metadata about the tables in the storage.
    pub fn db_tables_stats(&self) -> StorageResult<DbStats> {
        let mut tables_stats = BTreeMap::new();
        for name in Tables::field_names() {
            tables_stats.insert(name.to_string(), self.db_reader.get_table_stats(name)?);
        }
        Ok(DbStats { db_stats: self.db_reader.get_db_stats()?, tables_stats })
    }

    /// Returns the scope of the storage.
    pub fn get_scope(&self) -> StorageScope {
        self.scope
    }
}

/// A struct for starting RW transactions ([`StorageTxn`]) to the storage.
/// There is a single non clonable writer instance, to make sure there is only one write transaction
/// at any given moment.
pub struct StorageWriter {
    db_writer: DbWriter,
    file_writers: FileHandlers<RW>,
    tables: Arc<Tables>,
    scope: StorageScope,
}

impl StorageWriter {
    /// Takes a snapshot of the current state of the storage and returns a [`StorageTxn`] for
    /// reading and modifying data in the storage.
    pub fn begin_rw_txn(&mut self) -> StorageResult<StorageTxn<'_, RW>> {
        Ok(StorageTxn {
            txn: self.db_writer.begin_rw_txn()?,
            file_handlers: self.file_writers.clone(),
            tables: self.tables.clone(),
            scope: self.scope,
        })
    }
}

/// A struct for interacting with the storage.
/// The actually functionality is implemented on the transaction in multiple traits.
pub struct StorageTxn<'env, Mode: TransactionKind> {
    txn: DbTransaction<'env, Mode>,
    file_handlers: FileHandlers<Mode>,
    tables: Arc<Tables>,
    scope: StorageScope,
}

impl<'env> StorageTxn<'env, RW> {
    /// Commits the changes made in the transaction to the storage.
    pub fn commit(self) -> StorageResult<()> {
        self.file_handlers.flush();
        Ok(self.txn.commit()?)
    }
}

impl<'env, Mode: TransactionKind> StorageTxn<'env, Mode> {
    pub(crate) fn open_table<K: Key + Debug, V: ValueSerde + Debug>(
        &self,
        table_id: &TableIdentifier<K, V, SimpleTable>,
    ) -> StorageResult<TableHandle<'_, K, V, SimpleTable>> {
        if self.scope == StorageScope::StateOnly {
            let unused_tables = [
                self.tables.events.name,
                self.tables.transaction_hash_to_idx.name,
                self.tables.transaction_idx_to_hash.name,
                self.tables.transaction_outputs.name,
                self.tables.transactions.name,
            ];
            if unused_tables.contains(&table_id.name) {
                return Err(StorageError::ScopeError {
                    table_name: table_id.name.to_owned(),
                    storage_scope: self.scope,
                });
            }
        }
        Ok(self.txn.open_table(table_id)?)
    }
}

/// Returns the names of the tables in the storage.
pub fn table_names() -> &'static [&'static str] {
    Tables::field_names()
}

struct_field_names! {
    struct Tables {
        block_hash_to_number: TableIdentifier<BlockHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>,
        block_signatures: TableIdentifier<BlockNumber, NoVersionValueWrapper<BlockSignature>, SimpleTable>,
        casms: TableIdentifier<ClassHash, NoVersionValueWrapper<LocationInFile>, SimpleTable>,
        contract_storage: TableIdentifier<(ContractAddress, StorageKey, BlockNumber), NoVersionValueWrapper<Felt>, SimpleTable>,
        declared_classes: TableIdentifier<ClassHash, NoVersionValueWrapper<LocationInFile>, SimpleTable>,
        declared_classes_block: TableIdentifier<ClassHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>,
        deprecated_declared_classes: TableIdentifier<ClassHash, NoVersionValueWrapper<IndexedDeprecatedContractClass>, SimpleTable>,
        deployed_contracts: TableIdentifier<(ContractAddress, BlockNumber), NoVersionValueWrapper<ClassHash>, SimpleTable>,
        events: TableIdentifier<(ContractAddress, EventIndex), NoVersionValueWrapper<EventContent>, SimpleTable>,
        headers: TableIdentifier<BlockNumber, NoVersionValueWrapper<BlockHeader>, SimpleTable>,
        markers: TableIdentifier<MarkerKind, NoVersionValueWrapper<BlockNumber>, SimpleTable>,
        nonces: TableIdentifier<(ContractAddress, BlockNumber), NoVersionValueWrapper<Nonce>, SimpleTable>,
        file_offsets: TableIdentifier<OffsetKind, NoVersionValueWrapper<usize>, SimpleTable>,
        state_diffs: TableIdentifier<BlockNumber, NoVersionValueWrapper<LocationInFile>, SimpleTable>,
        transaction_hash_to_idx: TableIdentifier<TransactionHash, NoVersionValueWrapper<TransactionIndex>, SimpleTable>,
        transaction_idx_to_hash: TableIdentifier<TransactionIndex, NoVersionValueWrapper<TransactionHash>, SimpleTable>,
        transaction_outputs: TableIdentifier<TransactionIndex, NoVersionValueWrapper<ThinTransactionOutput>, SimpleTable>,
        transactions: TableIdentifier<TransactionIndex, NoVersionValueWrapper<Transaction>, SimpleTable>,

        // Version tables
        starknet_version: TableIdentifier<BlockNumber, NoVersionValueWrapper<StarknetVersion>, SimpleTable>,
        storage_version: TableIdentifier<String, NoVersionValueWrapper<Version>, SimpleTable>
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

// TODO: sort the variants alphabetically.
/// Error type for the storage crate.
#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    /// Errors related to the underlying database.
    #[error(transparent)]
    InnerError(#[from] DbError),
    #[error("Marker mismatch (expected {expected}, found {found}).")]
    MarkerMismatch { expected: BlockNumber, found: BlockNumber },
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
    /// Errors related to the underlying files.
    #[error(transparent)]
    MMapFileError(#[from] MMapFileError),
    #[error(transparent)]
    StorageVersionInconsistency(#[from] StorageVersionError),
    #[error("The table {table_name} is unused under the {storage_scope:?} storage scope.")]
    ScopeError { table_name: String, storage_scope: StorageScope },
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    #[error(
        "The block number {block} should be smaller than the compiled_class_marker \
         {compiled_class_marker}."
    )]
    InvalidBlockNumber { block: BlockNumber, compiled_class_marker: BlockNumber },
    #[error(
        "Attempt to write block signature {block_signature:?} of non-existing block \
         {block_number}."
    )]
    BlockSignatureForNonExistingBlock { block_number: BlockNumber, block_signature: BlockSignature },
}

/// A type alias that maps to std::result::Result<T, StorageError>.
pub type StorageResult<V> = std::result::Result<V, StorageError>;

/// A struct for the configuration of the storage.
#[allow(missing_docs)]
#[derive(Serialize, Debug, Default, Deserialize, Clone, PartialEq, Validate)]
pub struct StorageConfig {
    #[validate]
    pub db_config: DbConfig,
    #[validate]
    pub mmap_file_config: MmapFileConfig,
    pub scope: StorageScope,
}

impl SerializeConfig for StorageConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut dumped_config = BTreeMap::from_iter([ser_param(
            "scope",
            &self.scope,
            "The categories of data saved in storage.",
            ParamPrivacyInput::Public,
        )]);
        dumped_config
            .extend(append_sub_config_name(self.mmap_file_config.dump(), "mmap_file_config"));
        dumped_config.extend(append_sub_config_name(self.db_config.dump(), "db_config"));
        dumped_config
    }
}

/// A struct for the statistics of the tables in the database.
#[derive(Serialize, Deserialize, Debug)]
pub struct DbStats {
    /// Stats about the whole database.
    pub db_stats: DbWholeStats,
    /// A mapping from a table name in the database to its statistics.
    pub tables_stats: BTreeMap<String, DbTableStats>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
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

pub(crate) type MarkersTable<'env> =
    TableHandle<'env, MarkerKind, NoVersionValueWrapper<BlockNumber>, SimpleTable>;

#[derive(Clone, Debug)]
struct FileHandlers<Mode: TransactionKind> {
    thin_state_diff: FileHandler<NoVersionValueWrapper<ThinStateDiff>, Mode>,
    contract_class: FileHandler<NoVersionValueWrapper<ContractClass>, Mode>,
    casm: FileHandler<NoVersionValueWrapper<CasmContractClass>, Mode>,
    deprecated_contract_class: FileHandler<NoVersionValueWrapper<DeprecatedContractClass>, Mode>,
}

impl FileHandlers<RW> {
    // Appends a thin state diff to the corresponding file and returns its location.
    fn append_thin_state_diff(&self, thin_state_diff: &ThinStateDiff) -> LocationInFile {
        self.clone().thin_state_diff.append(thin_state_diff)
    }

    // Appends a contract class to the corresponding file and returns its location.
    fn append_contract_class(&self, contract_class: &ContractClass) -> LocationInFile {
        self.clone().contract_class.append(contract_class)
    }

    // Appends a CASM to the corresponding file and returns its location.
    fn append_casm(&self, casm: &CasmContractClass) -> LocationInFile {
        self.clone().casm.append(casm)
    }

    // Appends a deprecated contract class to the corresponding file and returns its location.
    fn append_deprecated_contract_class(
        &self,
        deprecated_contract_class: &DeprecatedContractClass,
    ) -> LocationInFile {
        self.clone().deprecated_contract_class.append(deprecated_contract_class)
    }

    // TODO(dan): Consider 1. flushing only the relevant files, 2. flushing concurrently.
    fn flush(&self) {
        self.thin_state_diff.flush();
        self.contract_class.flush();
        self.casm.flush();
        self.deprecated_contract_class.flush();
    }
}

impl<Mode: TransactionKind> FileHandlers<Mode> {
    // Returns the thin state diff at the given location or an error in case it doesn't exist.
    fn get_thin_state_diff_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<ThinStateDiff> {
        self.thin_state_diff.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("ThinStateDiff at location {:?} not found.", location),
        })
    }

    // Returns the contract class at the given location or an error in case it doesn't exist.
    fn get_contract_class_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<ContractClass> {
        self.contract_class.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("ContractClass at location {:?} not found.", location),
        })
    }

    // Returns the CASM at the given location or an error in case it doesn't exist.
    fn get_casm_unchecked(&self, location: LocationInFile) -> StorageResult<CasmContractClass> {
        self.casm.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("CasmContractClass at location {:?} not found.", location),
        })
    }

    // Returns the deprecated contract class at the given location or an error in case it doesn't
    // exist.
    fn get_deprecated_contract_class_unchecked(
        &self,
        location: LocationInFile,
    ) -> StorageResult<DeprecatedContractClass> {
        self.deprecated_contract_class.get(location)?.ok_or(StorageError::DBInconsistency {
            msg: format!("DeprecatedContractClass at location {:?} not found.", location),
        })
    }
}

fn open_storage_files(
    db_config: &DbConfig,
    mmap_file_config: MmapFileConfig,
    db_reader: DbReader,
    file_offsets_table: &TableIdentifier<OffsetKind, NoVersionValueWrapper<usize>, SimpleTable>,
) -> StorageResult<(FileHandlers<RW>, FileHandlers<RO>)> {
    let db_transaction = db_reader.begin_ro_txn()?;
    let table = db_transaction.open_table(file_offsets_table)?;

    let thin_state_diff_offset =
        table.get(&db_transaction, &OffsetKind::ThinStateDiff)?.unwrap_or_default();
    let (thin_state_diff_writer, thin_state_diff_reader) = open_file(
        mmap_file_config.clone(),
        db_config.path().join("thin_state_diff.dat"),
        thin_state_diff_offset,
    )?;

    let contract_class_offset =
        table.get(&db_transaction, &OffsetKind::ContractClass)?.unwrap_or_default();
    let (contract_class_writer, contract_class_reader) = open_file(
        mmap_file_config.clone(),
        db_config.path().join("contract_class.dat"),
        contract_class_offset,
    )?;

    let casm_offset = table.get(&db_transaction, &OffsetKind::Casm)?.unwrap_or_default();
    let (casm_writer, casm_reader) =
        open_file(mmap_file_config.clone(), db_config.path().join("casm.dat"), casm_offset)?;

    let deprecated_contract_class_offset =
        table.get(&db_transaction, &OffsetKind::DeprecatedContractClass)?.unwrap_or_default();
    let (deprecated_contract_class_writer, deprecated_contract_class_reader) = open_file(
        mmap_file_config,
        db_config.path().join("deprecated_contract_class.dat"),
        deprecated_contract_class_offset,
    )?;

    Ok((
        FileHandlers {
            thin_state_diff: thin_state_diff_writer,
            contract_class: contract_class_writer,
            casm: casm_writer,
            deprecated_contract_class: deprecated_contract_class_writer,
        },
        FileHandlers {
            thin_state_diff: thin_state_diff_reader,
            contract_class: contract_class_reader,
            casm: casm_reader,
            deprecated_contract_class: deprecated_contract_class_reader,
        },
    ))
}

/// Represents a kind of mmap file.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
pub enum OffsetKind {
    /// A thin state diff file.
    ThinStateDiff,
    /// A contract class file.
    ContractClass,
    /// A CASM file.
    Casm,
    /// A deprecated contract class file.
    DeprecatedContractClass,
}
