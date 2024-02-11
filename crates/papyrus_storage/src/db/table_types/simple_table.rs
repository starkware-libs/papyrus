#[cfg(test)]
#[path = "simple_table_test.rs"]
mod simple_table_test;

use std::borrow::Cow;
use std::fmt::Debug;
use std::marker::PhantomData;

use libmdbx::{TableFlags, WriteFlags};

use super::{DbResult, Table, TableType};
use crate::db::serialization::{Key as KeyTrait, ValueSerde};
use crate::db::table_types::DbCursorTrait;
use crate::db::{
    DbCursor,
    DbError,
    DbKeyType,
    DbTransaction,
    DbValueType,
    DbWriter,
    KeyAlreadyExistsError,
    TableHandle,
    TableIdentifier,
    TransactionKind,
    RW,
};

// A simple mapping between key and value.
pub(crate) struct SimpleTable;

impl TableType for SimpleTable {}

impl DbWriter {
    pub(crate) fn create_simple_table<K: KeyTrait + Debug, V: ValueSerde + Debug>(
        &mut self,
        name: &'static str,
    ) -> DbResult<TableIdentifier<K, V, SimpleTable>> {
        let txn = self.env.begin_rw_txn()?;
        txn.create_table(Some(name), TableFlags::empty())?;
        txn.commit()?;
        Ok(TableIdentifier {
            name,
            _key_type: PhantomData {},
            _value_type: PhantomData {},
            _table_type: PhantomData {},
        })
    }
}

impl<'env, K: KeyTrait + Debug, V: ValueSerde + Debug> Table<'env>
    for TableHandle<'env, K, V, SimpleTable>
{
    type Key = K;
    type Value = V;
    type TableVariant = SimpleTable;

    fn cursor<'txn, Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, Self::Key, Self::Value, SimpleTable>> {
        let cursor = txn.txn.cursor(&self.database)?;
        Ok(DbCursor {
            cursor,
            _key_type: PhantomData {},
            _value_type: PhantomData {},
            _table_type: PhantomData {},
        })
    }

    fn get<Mode: TransactionKind>(
        &'env self,
        txn: &'env DbTransaction<'env, Mode>,
        key: &Self::Key,
    ) -> DbResult<Option<<Self::Value as ValueSerde>::Value>> {
        // TODO: Support zero-copy. This might require a return type of Cow<'env, ValueType>.
        let bin_key = key.serialize()?;
        let Some(bytes) = txn.txn.get::<Cow<'env, [u8]>>(&self.database, &bin_key)? else {
            return Ok(None);
        };
        let value =
            <Self::Value>::deserialize(&mut bytes.as_ref()).ok_or(DbError::InnerDeserialization)?;
        Ok(Some(value))
    }

    fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()> {
        let data = <Self::Value>::serialize(value)?;
        let bin_key = key.serialize()?;
        txn.txn.put(&self.database, bin_key, data, WriteFlags::UPSERT)?;
        Ok(())
    }

    fn insert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()> {
        let data = <Self::Value>::serialize(value)?;
        let bin_key = key.serialize()?;
        txn.txn.put(&self.database, bin_key, data, WriteFlags::NO_OVERWRITE).map_err(|err| {
            match err {
                libmdbx::Error::KeyExist => {
                    DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(self.name, key, value))
                }
                _ => err.into(),
            }
        })?;
        Ok(())
    }

    #[allow(dead_code)]
    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &Self::Key) -> DbResult<()> {
        let bin_key = key.serialize()?;
        txn.txn.del(&self.database, bin_key, None)?;
        Ok(())
    }
}

impl<'env, K: KeyTrait + Debug, V: ValueSerde + Debug> TableHandle<'env, K, V, SimpleTable> {
    // Append key value pair to the end of the table. The key must be the last in the table,
    // otherwise an error will be returned.
    #[allow(dead_code)]
    pub(crate) fn append(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &K,
        value: &<V as ValueSerde>::Value,
    ) -> DbResult<()> {
        let data = V::serialize(value)?;
        let bin_key = key.serialize()?;
        txn.txn
            .put(&self.database, bin_key, data, WriteFlags::APPEND)
            .map_err(|_| DbError::Append)?;
        Ok(())
    }
}

impl<'txn, Mode: TransactionKind, K: KeyTrait + Debug, V: ValueSerde + Debug> DbCursorTrait
    for DbCursor<'txn, Mode, K, V, SimpleTable>
{
    type Key = K;
    type Value = V;

    fn prev(&mut self) -> DbResult<Option<(K, <Self::Value as ValueSerde>::Value)>> {
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

    fn next(&mut self) -> DbResult<Option<(K, <Self::Value as ValueSerde>::Value)>> {
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
    fn lower_bound(
        &mut self,
        key: &K,
    ) -> DbResult<Option<(K, <Self::Value as ValueSerde>::Value)>> {
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
