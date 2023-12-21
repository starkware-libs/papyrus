use std::borrow::Cow;
use std::fmt::Debug;
use std::marker::PhantomData;

use libmdbx::{TableFlags, WriteFlags};

use super::{DbResult, StorageSerde, Table};
use crate::db::serialization::StorageSerdeEx;
use crate::db::{
    DbCursor,
    DbCursorTrait,
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

// TODO(dvir): add option for this type when the key suffix and value are fixed size.

/// A table with keys with common prefix. The same common prefix will be saved only once.
pub struct CommonPrefix;

impl DbWriter {
    #[allow(dead_code)]
    pub(crate) fn create_common_prefix_table<
        K0: StorageSerde + Debug,
        K1: StorageSerde + Debug,
        V: StorageSerde + Debug,
    >(
        &mut self,
        name: &'static str,
    ) -> DbResult<TableIdentifier<(K0, K1), V, CommonPrefix>>
    where
        (K0, K1): StorageSerde + Debug,
    {
        let txn = self.env.begin_rw_txn()?;
        txn.create_table(Some(name), TableFlags::DUP_SORT)?;
        txn.commit()?;
        Ok(TableIdentifier {
            name,
            _key_type: PhantomData {},
            _value_type: PhantomData {},
            _table_type: PhantomData {},
        })
    }
}

impl<'env, K0: StorageSerde + Debug, K1: StorageSerde + Debug, V: StorageSerde + Debug + Default>
    Table<'env> for TableHandle<'env, (K0, K1), V, CommonPrefix>
where
    (K0, K1): StorageSerde + Debug,
{
    type Key = (K0, K1);
    type Value = V;
    type TableType = CommonPrefix;

    fn cursor<'txn, Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, Self::Key, Self::Value, CommonPrefix>> {
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
    ) -> DbResult<Option<Self::Value>> {
        let key_prefix = key.0.serialize()?;
        let key_suffix = key.1.serialize()?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        let Some(bytes) = cursor.get_both_range::<Cow<'_, [u8]>>(&key_prefix, &key_suffix)? else {
            return Ok(None);
        };
        if let Some(mut bytes) = bytes.strip_prefix(key_suffix.as_slice()) {
            let value = V::deserialize(&mut bytes).ok_or(DbError::InnerDeserialization)?;
            return Ok(Some(value));
        }
        Ok(None)
    }

    fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &Self::Value,
    ) -> DbResult<()> {
        let key_prefix = key.0.serialize()?;
        let key_suffix = key.1.serialize()?;
        let key_suffix_value = serialize_two(&key.1, value)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        cursor.put(&key_prefix, &key_suffix_value, WriteFlags::UPSERT)?;

        let mut cloned_cursor = cursor.clone();
        if let Some((_key_prefix, key_suffix_value)) =
            cloned_cursor.next_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if key_suffix_value.starts_with(&key_suffix) {
                cloned_cursor.del(WriteFlags::empty())?;
                return Ok(());
            }
        };

        if let Some((_key_prefix, key_suffix_value)) =
            cursor.prev_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if key_suffix_value.starts_with(&key_suffix) {
                cursor.del(WriteFlags::empty())?;
                return Ok(());
            }
        };
        Ok(())
    }

    fn insert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &Self::Value,
    ) -> DbResult<()> {
        let key_prefix = key.0.serialize()?;
        let key_suffix = key.1.serialize()?;
        let key_suffix_value = serialize_two(&key.1, value)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        cursor.put(&key_prefix, &key_suffix_value, WriteFlags::NO_DUP_DATA).map_err(
            |err| match err {
                libmdbx::Error::KeyExist => {
                    DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(self.name, key, value))
                }
                _ => err.into(),
            },
        )?;

        if let Some((_key_prefix, key_suffix_value)) =
            cursor.clone().next_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if key_suffix_value.starts_with(&key_suffix) {
                cursor.del(WriteFlags::empty())?;
                return Err(DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(
                    self.name, key, value,
                )));
            }
        };

        if let Some((_key_prefix, key_suffix_value)) =
            cursor.clone().prev_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if key_suffix_value.starts_with(&key_suffix) {
                cursor.del(WriteFlags::empty())?;
                return Err(DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(
                    self.name, key, value,
                )));
            }
        };
        Ok(())
    }

    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &Self::Key) -> DbResult<()> {
        let key_prefix = key.0.serialize()?;
        let key_suffix = key.1.serialize()?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        let Some(bytes) = cursor.get_both_range::<Cow<'_, [u8]>>(&key_prefix, &key_suffix)? else {
            return Ok(());
        };
        if bytes.starts_with(&key_suffix) {
            cursor.del(WriteFlags::empty())?;
        }
        Ok(())
    }
}

impl<
    'txn,
    Mode: TransactionKind,
    K0: StorageSerde + Debug,
    K1: StorageSerde + Debug,
    V: StorageSerde + Debug,
> DbCursorTrait for DbCursor<'txn, Mode, (K0, K1), V, CommonPrefix>
where
    (K0, K1): StorageSerde + Debug,
    (K1, V): StorageSerde + Debug,
{
    type Key = (K0, K1);
    type Value = V;

    fn prev(&mut self) -> DbResult<Option<(Self::Key, V)>> {
        let prev_cursor_res = self.cursor.prev::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_prefix_bytes, key_suffix_value_bytes)) => {
                let key_prefix = K0::deserialize(&mut key_prefix_bytes.as_ref())
                    .ok_or(DbError::InnerDeserialization)?;
                let (key_suffix, value) =
                    <(K1, Self::Value)>::deserialize(&mut key_suffix_value_bytes.as_ref())
                        .ok_or(DbError::InnerDeserialization)?;

                Ok(Some(((key_prefix, key_suffix), value)))
            }
        }
    }

    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> DbResult<Option<(Self::Key, V)>> {
        let prev_cursor_res = self.cursor.next::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((key_prefix_bytes, key_suffix_value_bytes)) => {
                let key_prefix = K0::deserialize(&mut key_prefix_bytes.as_ref())
                    .ok_or(DbError::InnerDeserialization)?;
                let (key_suffix, value) =
                    <(K1, Self::Value)>::deserialize(&mut key_suffix_value_bytes.as_ref())
                        .ok_or(DbError::InnerDeserialization)?;

                Ok(Some(((key_prefix, key_suffix), value)))
            }
        }
    }

    // TODO(dvir): make this function walk only once on the table and not two. This will
    // require add functionality of libmdbx to the binding or use assumption on the current
    // implementation of the binding.
    /// Position at first key greater than or equal to specified key.
    fn lower_bound(&mut self, key: &Self::Key) -> DbResult<Option<(Self::Key, Self::Value)>> {
        let mut key_prefix = key.0.serialize()?;
        let key_suffix = key.1.serialize()?;

        // First try to find a match for the key prefix.
        if let Some(value_bytes) =
            self.cursor.get_both_range::<DbValueType<'_>>(&key_prefix, &key_suffix)?
        {
            let (second_key, value) = <(K1, V)>::deserialize(&mut value_bytes.as_ref())
                .ok_or(DbError::InnerDeserialization)?;

            // A trick to get own copy of the key.
            let first_key = K0::deserialize(&mut key.0.serialize()?.as_slice())
                .ok_or(DbError::InnerDeserialization)?;
            return Ok(Some(((first_key, second_key), value)));
        }

        // The first key prefix bytes that greater than the current key prefix bytes.
        add_one(&mut key_prefix);

        let Some((key_prefix_bytes, key_suffix_value_bytes)) =
            self.cursor.set_range::<DbKeyType<'_>, DbValueType<'_>>(&key_prefix)?
        else {
            return Ok(None);
        };

        let (second_key, value) = <(K1, V)>::deserialize(&mut key_suffix_value_bytes.as_ref())
            .ok_or(DbError::InnerDeserialization)?;

        let first_key =
            K0::deserialize(&mut key_prefix_bytes.as_ref()).ok_or(DbError::InnerDeserialization)?;

        Ok(Some(((first_key, second_key), value)))
    }
}

fn serialize_two<V1: StorageSerde, V2: StorageSerde>(v1: &V1, v2: &V2) -> DbResult<Vec<u8>> {
    let mut res = Vec::new();
    v1.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
    v2.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
    Ok(res)
}

// Adds one to the number represented by the bytes.
fn add_one(bytes: &mut Vec<u8>) {
    let mut carry = true;
    for current in bytes.iter_mut().rev() {
        if carry {
            *current += 1;
            carry = *current == 0;
        }
    }
    if carry {
        bytes.insert(0, 1);
    }
}

#[cfg(test)]
mod common_prefix_test {
    use crate::db::db_test::get_test_env;
    use crate::db::table_types::test_utils::{table_cursor_test, table_test};

    #[test]
    fn common_prefix_table_test() {
        let ((reader, mut writer), _temp_dir) = get_test_env();
        let table_id = writer.create_common_prefix_table("table").unwrap();
        table_test(table_id, &reader, &mut writer);
    }

    #[test]
    fn common_prefix_table_cursor_test() {
        let ((reader, mut writer), _temp_dir) = get_test_env();
        let table_id = writer.create_common_prefix_table("table").unwrap();
        table_cursor_test(table_id, &reader, &mut writer);
    }
}
