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

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::result;
use std::sync::Arc;

use libmdbx::{Cursor, Geometry, TableFlags, WriteFlags, WriteMap};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::validators::validate_ascii;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

use crate::db::serialization::{StorageSerde, StorageSerdeEx};

// Maximum number of Sub-Databases.
const MAX_DBS: usize = 27;

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
    pub path_prefix: PathBuf,
    /// The [chain id](https://docs.rs/starknet_api/latest/starknet_api/core/struct.ChainId.html) of the Starknet network.
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
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
            chain_id: ChainId("SN_MAIN".to_string()),
            min_size: 1 << 20,    // 1MB
            max_size: 1 << 40,    // 1TB
            growth_step: 1 << 26, // 64MB
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
        self.path_prefix.join(self.chain_id.0.as_str())
    }
}

/// An error that can occur when interacting with the database.
#[derive(thiserror::Error, Debug)]
pub enum DbError {
    /// An error that occurred in the database library.
    #[error(transparent)]
    Inner(#[from] libmdbx::Error),
    #[error("Deserialization failed.")]
    /// An error that occurred during deserialization.
    InnerDeserialization,
    /// An error that occurred during serialization.
    #[error("Serialization failed.")]
    Serialization,
}
type DbResult<V> = result::Result<V, DbError>;

/// Tries to open an MDBX environment and returns a reader and a writer to it.
/// There is a single non clonable writer instance, to make sure there is only one write transaction
///  at any given moment.
pub(crate) fn open_env(config: DbConfig) -> DbResult<(DbReader, DbWriter)> {
    let env = Arc::new(
        Environment::new()
            .set_geometry(Geometry {
                size: Some(config.min_size..config.max_size),
                growth_step: Some(config.growth_step),
                ..Default::default()
            })
            .set_max_tables(MAX_DBS)
            .open(&config.path())?,
    );
    Ok((DbReader { env: env.clone() }, DbWriter { env }))
}

#[derive(Clone)]
pub(crate) struct DbReader {
    env: Arc<Environment>,
}

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

    pub(crate) fn create_table<K: StorageSerde, V: StorageSerde>(
        &mut self,
        name: &'static str,
    ) -> DbResult<TableIdentifier<K, V>> {
        let txn = self.env.begin_rw_txn()?;
        txn.create_table(Some(name), TableFlags::empty())?;
        txn.commit()?;
        Ok(TableIdentifier { name, _key_type: PhantomData {}, _value_type: PhantomData {} })
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
    pub fn open_table<'env, K: StorageSerde, V: StorageSerde>(
        &'env self,
        table_id: &TableIdentifier<K, V>,
    ) -> DbResult<TableHandle<'env, K, V>> {
        let database = self.txn.open_table(Some(table_id.name))?;
        Ok(TableHandle { database, _key_type: PhantomData {}, _value_type: PhantomData {} })
    }
}

pub(crate) struct TableIdentifier<K: StorageSerde, V: StorageSerde> {
    name: &'static str,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}

pub(crate) struct TableHandle<'env, K: StorageSerde, V: StorageSerde> {
    database: libmdbx::Table<'env>,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}

impl<'env, 'txn, K: StorageSerde, V: StorageSerde> TableHandle<'env, K, V> {
    pub(crate) fn cursor<Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, K, V>> {
        let cursor = txn.txn.cursor(&self.database)?;
        Ok(DbCursor { cursor, _key_type: PhantomData {}, _value_type: PhantomData {} })
    }

    pub(crate) fn get<Mode: TransactionKind>(
        &'env self,
        txn: &'env DbTransaction<'env, Mode>,
        key: &K,
    ) -> DbResult<Option<V>> {
        // TODO: Support zero-copy. This might require a return type of Cow<'env, ValueType>.
        let bin_key = key.serialize()?;
        let Some(bytes) = txn.txn.get::<Cow<'env, [u8]>>(&self.database, &bin_key)? else {
            return Ok(None);
        };
        let value = V::deserialize(&mut bytes.as_ref()).ok_or(DbError::InnerDeserialization)?;
        Ok(Some(value))
    }

    pub(crate) fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &K,
        value: &V,
    ) -> DbResult<()> {
        let data = value.serialize()?;
        let bin_key = key.serialize()?;
        txn.txn.put(&self.database, bin_key, data, WriteFlags::UPSERT)?;
        Ok(())
    }

    pub(crate) fn insert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &K,
        value: &V,
    ) -> DbResult<()> {
        let data = value.serialize()?;
        let bin_key = key.serialize()?;
        txn.txn.put(&self.database, bin_key, data, WriteFlags::NO_OVERWRITE)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &K) -> DbResult<()> {
        let bin_key = key.serialize()?;
        txn.txn.del(&self.database, bin_key, None)?;
        Ok(())
    }
}

pub(crate) struct DbCursor<'txn, Mode: TransactionKind, K: StorageSerde, V: StorageSerde> {
    cursor: Cursor<'txn, Mode::Internal>,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}

impl<'txn, Mode: TransactionKind, K: StorageSerde, V: StorageSerde> DbCursor<'txn, Mode, K, V> {
    pub(crate) fn prev(&mut self) -> DbResult<Option<(K, V)>> {
        let prev_cursor_res = self.cursor.prev::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_bytes, value_bytes)) => {
                let key =
                    K::deserialize(&mut key_bytes.as_ref()).ok_or(DbError::InnerDeserialization)?;
                let value = V::deserialize(&mut value_bytes.as_ref())
                    .ok_or(DbError::InnerDeserialization)?;
                Ok(Some((key, value)))
            }
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub(crate) fn next(&mut self) -> DbResult<Option<(K, V)>> {
        let prev_cursor_res = self.cursor.next::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_bytes, value_bytes)) => {
                let key =
                    K::deserialize(&mut key_bytes.as_ref()).ok_or(DbError::InnerDeserialization)?;
                let value = V::deserialize(&mut value_bytes.as_ref())
                    .ok_or(DbError::InnerDeserialization)?;
                Ok(Some((key, value)))
            }
        }
    }

    /// Position at first key greater than or equal to specified key.
    pub(crate) fn lower_bound(&mut self, key: &K) -> DbResult<Option<(K, V)>> {
        let key_bytes = key.serialize()?;
        let prev_cursor_res =
            self.cursor.set_range::<DbKeyType<'_>, DbValueType<'_>>(&key_bytes)?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_bytes, value_bytes)) => {
                let key =
                    K::deserialize(&mut key_bytes.as_ref()).ok_or(DbError::InnerDeserialization)?;
                let value = V::deserialize(&mut value_bytes.as_ref())
                    .ok_or(DbError::InnerDeserialization)?;
                Ok(Some((key, value)))
            }
        }
    }
}

#[doc(hidden)]
pub struct RO {}

impl TransactionKind for RO {
    type Internal = libmdbx::RO;
}

#[doc(hidden)]
pub struct RW {}

impl TransactionKind for RW {
    type Internal = libmdbx::RW;
}
