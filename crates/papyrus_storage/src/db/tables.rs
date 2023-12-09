use std::borrow::Cow;
use std::fmt::Debug;
use std::marker::PhantomData;

use libmdbx::{TableFlags, WriteFlags};

use super::{
    DbCursor,
    DbError,
    DbResult,
    DbTransaction,
    DbWriter,
    KeyAlreadyExistsError,
    Table,
    TableHandle,
    TableIdentifier,
    TransactionKind,
    RW,
};
use crate::db::serialization::{StorageSerde, StorageSerdeEx};

/// Simple mapping between key and value.
#[derive(Debug)]
pub struct Simple;

impl DbWriter {
    pub(crate) fn create_simple_table<K: StorageSerde + Debug, V: StorageSerde + Debug>(
        &mut self,
        name: &'static str,
    ) -> DbResult<TableIdentifier<K, V, Simple>> {
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

impl<'env, 'txn, K: StorageSerde + Debug, V: StorageSerde + Debug> Table<'env, 'txn, K, V>
    for TableHandle<'env, K, V, Simple>
{
    fn cursor<Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, K, V>> {
        let cursor = txn.txn.cursor(&self.database)?;
        Ok(DbCursor { cursor, _key_type: PhantomData {}, _value_type: PhantomData {} })
    }

    fn get<Mode: TransactionKind>(
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

    fn upsert(&'env self, txn: &DbTransaction<'env, RW>, key: &K, value: &V) -> DbResult<()> {
        let data = value.serialize()?;
        let bin_key = key.serialize()?;
        txn.txn.put(&self.database, bin_key, data, WriteFlags::UPSERT)?;
        Ok(())
    }

    fn insert(&'env self, txn: &DbTransaction<'env, RW>, key: &K, value: &V) -> DbResult<()> {
        let data = value.serialize()?;
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
    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &K) -> DbResult<()> {
        let bin_key = key.serialize()?;
        txn.txn.del(&self.database, bin_key, None)?;
        Ok(())
    }
}
