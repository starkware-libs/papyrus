//! Basic structs for interacting with the db.
//!
//! Low database layer for interaction with libmdbx. The API is supposedly generic enough to easily
//! replace the database library with other Berkley-like database implementations.
//!
//! Assumptions:
//! - The database is transactional with full ACID semantics.
//! - The keys are always sorted and range lookups are supported.
//!
//! Guarantees:
//! - The serialization is consistent across code versions (though, not necessarily across
//!   machines).

#[cfg(test)]
mod db_test;

/// Statistics and information about the database.
pub mod db_stats;
// TODO(yair): Make the serialization module pub(crate).
#[doc(hidden)]
pub mod serialization;
pub(crate) mod table_types;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::result;
use std::sync::Arc;

use libmdbx::{DatabaseFlags, Geometry, PageSize, WriteMap};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::validators::{validate_ascii, validate_path_exists};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

use self::serialization::{Key, ValueSerde};
use self::table_types::{DbCursor, DbCursorTrait};
use crate::db::table_types::TableType;

// Maximum number of Sub-Databases.
const MAX_DBS: usize = 18;

// Note that NO_TLS mode is used by default.
type EnvironmentKind = WriteMap;
type Environment = libmdbx::Database<EnvironmentKind>;

type DbKeyType<'env> = Cow<'env, [u8]>;
type DbValueType<'env> = Cow<'env, [u8]>;

/// The configuration of the database.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Validate)]
pub struct DbConfig {
    /// The path prefix of the database files. The final path is the path prefix followed by the
    /// chain id.
    #[validate(custom = "validate_path_exists")]
    pub path_prefix: PathBuf,
    /// The [chain id](https://docs.rs/starknet_api/latest/starknet_api/core/struct.ChainId.html) of the Starknet network.
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    /// Whether to enforce that the path exists. If true, `open_env` fails when the mdbx.dat file
    /// does not exist.
    pub enforce_file_exists: bool,
    /// The minimum size of the database.
    pub min_size: usize,
    /// The maximum size of the database.
    pub max_size: usize,
    /// The growth step of the database.
    pub growth_step: isize,
}

impl Default for DbConfig {
    fn default() -> Self {
        DbConfig {
            path_prefix: PathBuf::from("./data"),
            chain_id: ChainId::Mainnet,
            enforce_file_exists: false,
            min_size: 1 << 20,    // 1MB
            max_size: 1 << 40,    // 1TB
            growth_step: 1 << 32, // 4GB
        }
    }
}

impl SerializeConfig for DbConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "path_prefix",
                &self.path_prefix,
                "Prefix of the path of the node's storage directory, the storage file path \
                will be <path_prefix>/<chain_id>. The path is not created automatically.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "enforce_file_exists",
                &self.enforce_file_exists,
                "Whether to enforce that the path exists. If true, `open_env` fails when the \
                mdbx.dat file does not exist.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_size",
                &self.min_size,
                "The minimum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_size",
                &self.max_size,
                "The maximum size of the node's storage in bytes.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "growth_step",
                &self.growth_step,
                "The growth step in bytes, must be greater than zero to allow the database to \
                 grow.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl DbConfig {
    /// Returns the path of the database (path prefix, followed by the chain id).
    pub fn path(&self) -> PathBuf {
        self.path_prefix.join(self.chain_id.to_string().as_str())
    }
}

/// An error that can occur when interacting with the database.
#[derive(thiserror::Error, Debug)]
pub enum DbError {
    /// An error that occurred in the database library.
    #[error(transparent)]
    Inner(#[from] libmdbx::Error),
    /// An error that occurred when tried to insert a key that already exists in a table.
    #[error(
        "Key '{}' already exists in table '{}'. Error when tried to insert value '{}'", .0.key,
        .0.table_name, .0.value
    )]
    KeyAlreadyExists(KeyAlreadyExistsError),
    #[error("Deserialization failed.")]
    /// An error that occurred during deserialization.
    InnerDeserialization,
    /// An error that occurred during serialization.
    #[error("Serialization failed.")]
    Serialization,
    /// An error that occurred when trying to open a db file that does not exist.
    #[error("The file '{0}' does not exist.")]
    FileDoesNotExist(PathBuf),
    // TODO(dvir): consider adding more details about the error, table name, key, value and last
    // key in the tree.
    /// An error that occurred when trying to append a key when it is not the last.
    #[error("Append error. The key is not the last in the table.")]
    Append,
}

type DbResult<V> = result::Result<V, DbError>;

/// A helper struct for DbError::KeyAlreadyExists.
#[derive(Debug)]
pub struct KeyAlreadyExistsError {
    /// The name of the table.
    pub table_name: &'static str,
    /// The key that already exists in the table.
    pub key: String,
    /// The value that was tried to be inserted.
    pub value: String,
}

impl KeyAlreadyExistsError {
    /// Creates a new KeyAlreadyExistsError.
    pub fn new(table_name: &'static str, key: &impl Debug, value: &impl Debug) -> Self {
        Self { table_name, key: format!("{:?}", key), value: format!("{:?}", value) }
    }
}

/// Tries to open an MDBX environment and returns a reader and a writer to it.
/// There is a single non clonable writer instance, to make sure there is only one write transaction
///  at any given moment.
pub(crate) fn open_env(config: &DbConfig) -> DbResult<(DbReader, DbWriter)> {
    let db_file_path = config.path().join("mdbx.dat");
    // Checks if path exists if enforce_file_exists is true.
    if config.enforce_file_exists && !db_file_path.exists() {
        return Err(DbError::FileDoesNotExist(db_file_path));
    }
    // const MAX_READERS: u32 = 1 << 13; // 8K readers
    const MAX_READERS: u32 = 200000; // 200K readers
    let env = Arc::new(
        Environment::new()
            .set_geometry(Geometry {
                size: Some(config.min_size..config.max_size),
                growth_step: Some(config.growth_step),
                page_size: Some(get_page_size(page_size::get())),
                ..Default::default()
            })
            .set_max_tables(MAX_DBS)
            .set_max_readers(MAX_READERS)
            .set_flags(DatabaseFlags { no_rdahead: true, liforeclaim: true, ..Default::default() })
            .open(&config.path())?,
    );
    Ok((DbReader { env: env.clone() }, DbWriter { env }))
}

// Size in bytes.
const MDBX_MIN_PAGESIZE: usize = 256;
const MDBX_MAX_PAGESIZE: usize = 65536; // 64KB

fn get_page_size(os_page_size: usize) -> PageSize {
    let mut page_size = os_page_size.clamp(MDBX_MIN_PAGESIZE, MDBX_MAX_PAGESIZE);

    // Page size must be power of two.
    if !page_size.is_power_of_two() {
        page_size = page_size.next_power_of_two() / 2;
    }

    PageSize::Set(page_size)
}

#[derive(Clone, Debug)]
pub(crate) struct DbReader {
    env: Arc<Environment>,
}

#[derive(Debug)]
pub(crate) struct DbWriter {
    env: Arc<Environment>,
}

impl DbReader {
    pub(crate) fn begin_ro_txn(&self) -> DbResult<DbReadTransaction<'_>> {
        Ok(DbReadTransaction { txn: self.env.begin_ro_txn()? })
    }
}

type DbReadTransaction<'env> = DbTransaction<'env, RO>;

impl DbWriter {
    pub(crate) fn begin_rw_txn(&mut self) -> DbResult<DbWriteTransaction<'_>> {
        Ok(DbWriteTransaction { txn: self.env.begin_rw_txn()? })
    }
}

type DbWriteTransaction<'env> = DbTransaction<'env, RW>;

impl<'a> DbWriteTransaction<'a> {
    pub(crate) fn commit(self) -> DbResult<()> {
        self.txn.commit()?;
        Ok(())
    }
}

#[doc(hidden)]
// Transaction wrappers.
pub trait TransactionKind {
    type Internal: libmdbx::TransactionKind;
}

pub(crate) struct DbTransaction<'env, Mode: TransactionKind> {
    txn: libmdbx::Transaction<'env, Mode::Internal, EnvironmentKind>,
}

impl<'a, Mode: TransactionKind> DbTransaction<'a, Mode> {
    pub fn open_table<'env, K: Key + Debug, V: ValueSerde + Debug, T: TableType>(
        &'env self,
        table_id: &TableIdentifier<K, V, T>,
    ) -> DbResult<TableHandle<'env, K, V, T>> {
        let database = self.txn.open_table(Some(table_id.name))?;
        Ok(TableHandle {
            database,
            name: table_id.name,
            _key_type: PhantomData {},
            _value_type: PhantomData {},
            _table_type: PhantomData {},
        })
    }
}
pub(crate) struct TableIdentifier<K: Key + Debug, V: ValueSerde + Debug, T: TableType> {
    pub(crate) name: &'static str,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
    _table_type: PhantomData<T>,
}

pub(crate) struct TableHandle<'env, K: Key + Debug, V: ValueSerde + Debug, T: TableType> {
    database: libmdbx::Table<'env>,
    name: &'static str,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
    _table_type: PhantomData<T>,
}

/// Iterator for iterating over a DB table
pub(crate) struct DbIter<'cursor, 'txn, Mode: TransactionKind, K: Key, V: ValueSerde, T: TableType>
{
    cursor: &'cursor mut DbCursor<'txn, Mode, K, V, T>,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}

impl<'cursor, 'txn, Mode: TransactionKind, K: Key, V: ValueSerde, T: TableType>
    DbIter<'cursor, 'txn, Mode, K, V, T>
{
    #[allow(dead_code)]
    pub(crate) fn new(cursor: &'cursor mut DbCursor<'txn, Mode, K, V, T>) -> Self {
        Self { cursor, _key_type: PhantomData {}, _value_type: PhantomData {} }
    }
}

impl<'cursor, 'txn, Mode: TransactionKind, K: Key, V: ValueSerde, T: TableType> Iterator
    for DbIter<'cursor, 'txn, Mode, K, V, T>
where
    DbCursor<'txn, Mode, K, V, T>: DbCursorTrait<Key = K, Value = V>,
{
    type Item = DbResult<(K, V::Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        let prev_cursor_res = self.cursor.next().transpose()?;
        Some(prev_cursor_res)
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct RO {}

impl TransactionKind for RO {
    type Internal = libmdbx::RO;
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct RW {}

impl TransactionKind for RW {
    type Internal = libmdbx::RW;
}
