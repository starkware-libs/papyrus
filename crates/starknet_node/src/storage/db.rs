#[cfg(test)]
#[path = "db_test.rs"]
pub mod db_test;

use std::borrow::Cow;
use std::marker::PhantomData;
use std::path::Path;
use std::result;
use std::sync::Arc;

use libmdbx::{Cursor, DatabaseFlags, Geometry, WriteFlags, WriteMap};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// Low database layer for interaction with libmdbx. The API is supposedly generic enough to easily
// replace the database library with other Berkley-like database implementations.
//
// Assumptions:
// The serialization is consistent across code versions (though, not necessarily across machines).

// Maximum number of Sub-Databases.
// TODO(spapini): Get these from configuration, and have a separate test configuration.
const MAX_DBS: usize = 10;
const MIN_SIZE: usize = 1 << 20; // Minimum db size 1MB;
const GROWTH_STEP: isize = 1 << 26; // Growth step 64MB;

// Note that NO_TLS mode is used by default.
type EnvironmentKind = WriteMap;
type Environment = libmdbx::Environment<EnvironmentKind>;

pub trait ValueType: Serialize + DeserializeOwned {}
impl<T: Serialize + DeserializeOwned> ValueType for T {}
type DbKeyType<'env> = Cow<'env, [u8]>;
type DbValueType<'env> = Cow<'env, [u8]>;

#[derive(Serialize, Deserialize)]
pub struct DbConfig {
    pub path: String,
    pub max_size: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error(transparent)]
    InnerDbError(#[from] libmdbx::Error),
    #[error(transparent)]
    DeserializationError(#[from] bincode::Error),
}
pub type Result<V> = result::Result<V, DbError>;

/// Opens an MDBX environment and returns a reader and a writer to it.
/// There is a single non clonable writer instance, to make sure there is only one write transaction
///  at any given moment.
pub fn open_env(config: DbConfig) -> Result<(DbReader, DbWriter)> {
    let env = Arc::new(
        Environment::new()
            .set_geometry(Geometry {
                size: Some(MIN_SIZE..config.max_size),
                growth_step: Some(GROWTH_STEP),
                ..Default::default()
            })
            .set_max_dbs(MAX_DBS)
            .open(Path::new(&config.path))?,
    );
    Ok((DbReader { env: env.clone() }, DbWriter { env }))
}

#[derive(Clone)]
pub struct DbReader {
    env: Arc<Environment>,
}
pub struct DbWriter {
    env: Arc<Environment>,
}

// Transaction wrappers.
pub trait TransactionKind {
    type Internal: libmdbx::TransactionKind;
}
pub struct RO {}
pub struct RW {}

impl TransactionKind for RO {
    type Internal = libmdbx::RO;
}
impl TransactionKind for RW {
    type Internal = libmdbx::RW;
}
pub struct DbTransaction<'env, Mode: TransactionKind> {
    txn: libmdbx::Transaction<'env, Mode::Internal, EnvironmentKind>,
}
pub type DbReadTransaction<'env> = DbTransaction<'env, RO>;
pub type DbWriteTransaction<'env> = DbTransaction<'env, RW>;

pub struct TableIdentifier<K: ValueType, V: ValueType> {
    name: &'static str,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}
pub struct TableHandle<'env, K: ValueType, V: ValueType> {
    database: libmdbx::Database<'env>,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}
impl<'env, 'txn, K: ValueType, V: ValueType> TableHandle<'env, K, V> {
    pub fn cursor<Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> Result<DbCursor<'txn, Mode, K, V>> {
        let cursor = txn.txn.cursor(&self.database)?;
        Ok(DbCursor { cursor, _key_type: PhantomData {}, _value_type: PhantomData {} })
    }
    pub fn get<Mode: TransactionKind>(
        &'env self,
        txn: &'env DbTransaction<'env, Mode>,
        key: &K,
    ) -> Result<Option<V>> {
        // TODO: Support zero-copy. This might require a return type of Cow<'env, ValueType>.
        let bin_key = bincode::serialize(key).unwrap();
        if let Some(bytes) = txn.txn.get::<Cow<'env, [u8]>>(&self.database, &bin_key)? {
            let value = bincode::deserialize::<V>(bytes.as_ref())?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    pub fn upsert(&'env self, txn: &DbTransaction<'env, RW>, key: &K, value: &V) -> Result<()> {
        let data = bincode::serialize::<V>(value).unwrap();
        let bin_key = bincode::serialize(key).unwrap();
        txn.txn.put(&self.database, &bin_key, &data, WriteFlags::UPSERT)?;
        Ok(())
    }
    pub fn insert(&'env self, txn: &DbTransaction<'env, RW>, key: &K, value: &V) -> Result<()> {
        let data = bincode::serialize::<V>(value).unwrap();
        let bin_key = bincode::serialize(key).unwrap();
        txn.txn.put(&self.database, &bin_key, &data, WriteFlags::NO_OVERWRITE)?;
        Ok(())
    }
    #[allow(dead_code)]
    pub fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &K) -> Result<()> {
        let bin_key = bincode::serialize(key).unwrap();
        txn.txn.del(&self.database, &bin_key, None)?;
        Ok(())
    }
}

impl DbReader {
    pub fn begin_ro_txn(&self) -> Result<DbReadTransaction<'_>> {
        Ok(DbReadTransaction { txn: self.env.begin_ro_txn()? })
    }
}
impl DbWriter {
    pub fn begin_rw_txn(&mut self) -> Result<DbWriteTransaction<'_>> {
        Ok(DbWriteTransaction { txn: self.env.begin_rw_txn()? })
    }
    pub fn create_table<K: ValueType, V: ValueType>(
        &mut self,
        name: &'static str,
    ) -> Result<TableIdentifier<K, V>> {
        let txn = self.env.begin_rw_txn()?;
        txn.create_db(Some(name), DatabaseFlags::empty())?;
        txn.commit()?;
        Ok(TableIdentifier { name, _key_type: PhantomData {}, _value_type: PhantomData {} })
    }
}

impl<'a, Mode: TransactionKind> DbTransaction<'a, Mode> {
    pub fn open_table<'env, K: ValueType, V: ValueType>(
        &'env self,
        table_id: &TableIdentifier<K, V>,
    ) -> Result<TableHandle<'env, K, V>> {
        let database = self.txn.open_db(Some(table_id.name))?;
        Ok(TableHandle { database, _key_type: PhantomData {}, _value_type: PhantomData {} })
    }
}
impl<'a> DbWriteTransaction<'a> {
    pub fn commit(self) -> Result<()> {
        self.txn.commit()?;
        Ok(())
    }
}

pub struct DbCursor<'txn, Mode: TransactionKind, K: ValueType, V: ValueType> {
    cursor: Cursor<'txn, Mode::Internal>,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
}

impl<'txn, Mode: TransactionKind, K: ValueType, V: ValueType> DbCursor<'txn, Mode, K, V> {
    pub fn prev(&mut self) -> Result<Option<(K, V)>> {
        let prev_cursor_res = self.cursor.prev::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_bytes, value_bytes)) => {
                let key = bincode::deserialize::<K>(key_bytes.as_ref())?;
                let value = bincode::deserialize::<V>(value_bytes.as_ref())?;
                Ok(Some((key, value)))
            }
        }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<(K, V)>> {
        let prev_cursor_res = self.cursor.next::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_bytes, value_bytes)) => {
                let key = bincode::deserialize::<K>(key_bytes.as_ref())?;
                let value = bincode::deserialize::<V>(value_bytes.as_ref())?;
                Ok(Some((key, value)))
            }
        }
    }
    pub fn lower_bound(&mut self, key: &K) -> Result<Option<(K, V)>> {
        let key_bytes = bincode::serialize(key).unwrap();
        let prev_cursor_res =
            self.cursor.set_range::<DbKeyType<'_>, DbValueType<'_>>(&key_bytes)?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_bytes, value_bytes)) => {
                let key = bincode::deserialize::<K>(key_bytes.as_ref())?;
                let value = bincode::deserialize::<V>(value_bytes.as_ref())?;
                Ok(Some((key, value)))
            }
        }
    }
}
