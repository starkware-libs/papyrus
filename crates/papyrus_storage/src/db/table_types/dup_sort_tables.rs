#[cfg(test)]
#[path = "dup_sort_tables_test.rs"]
mod dup_sort_tables_test;

use std::borrow::Cow;
use std::fmt::Debug;
use std::marker::PhantomData;

use libmdbx::{TableFlags, WriteFlags};

use super::const_serialization_size::ConstSerializationSize;
use super::{DbResult, Table, TableType};
use crate::db::serialization::{Key as KeyTrait, StorageSerde, StorageSerdeEx, ValueSerde};
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

// All tables types that use libmdbx dup sort feature
trait DupSortTableType {}
impl<T: DupSortTableType> TableType for T {}

// A table with keys with common prefix. The same common prefix will be saved only once.
// NOTICE: the size of the serialized sub key and value must be half of page size.
pub(crate) struct CommonPrefix;

// A table with keys with common prefix where the sub key and value are fixed size.
// NOTICE: the size of the serialized sub key and value must be half of page size.
pub(crate) struct CommonPrefixFixedSize;

impl DupSortTableType for CommonPrefix {}
impl DupSortTableType for CommonPrefixFixedSize {}

trait DupSortUtils<K: KeyTrait, V: ValueSerde> {
    // Returns the main key.
    fn get_main_key(key: &K) -> DbResult<Vec<u8>>;

    // Returns the sub key.
    fn get_sub_key(key: &K) -> DbResult<Vec<u8>>;

    // Returns the sub key and value bytes.
    fn get_sub_key_value(key: &K, value: &V::Value) -> DbResult<Vec<u8>>;

    // Returns the first key that is greater than or equal to the given key.
    fn get_first_key_after(key: &K) -> DbResult<Vec<u8>>;

    // Changes main_key_bytes to the next greater one.
    fn next_main_key(main_key_bytes: &mut Vec<u8>);

    // Returns a key value pair from main_key bytes and sub_key_value bytes.
    fn get_key_value_pair(main_key: &[u8], sub_key_value: &[u8]) -> Option<(K, V::Value)>;
}

// TODO(dvir): consider add test for the implementation.
impl<MainKey: KeyTrait, SubKey: KeyTrait, V: ValueSerde> DupSortUtils<(MainKey, SubKey), V>
    for CommonPrefix
where
    (MainKey, SubKey): KeyTrait,
{
    fn get_main_key(key: &(MainKey, SubKey)) -> DbResult<Vec<u8>> {
        key.0.serialize()
    }

    fn get_sub_key(key: &(MainKey, SubKey)) -> DbResult<Vec<u8>> {
        key.1.serialize()
    }

    fn get_sub_key_value<'a>(key: &(MainKey, SubKey), value: &V::Value) -> DbResult<Vec<u8>> {
        let mut res = Vec::new();
        key.1.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        value.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        Ok(res)
    }

    fn get_first_key_after(key: &(MainKey, SubKey)) -> DbResult<Vec<u8>> {
        key.1.serialize()
    }

    fn next_main_key(main_key_bytes: &mut Vec<u8>) {
        add_one(main_key_bytes);
    }

    fn get_key_value_pair(
        mut main_key: &[u8],
        mut sub_key_value: &[u8],
    ) -> Option<((MainKey, SubKey), V::Value)> {
        Some((
            (MainKey::deserialize(&mut main_key)?, SubKey::deserialize_from(&mut sub_key_value)?),
            V::Value::deserialize(&mut sub_key_value)?,
        ))
    }
}

impl<
    MainKey: KeyTrait,
    SubKey: KeyTrait + ConstSerializationSize,
    V: ValueSerde + ConstSerializationSize,
> DupSortUtils<(MainKey, SubKey), V> for CommonPrefixFixedSize
where
    (MainKey, SubKey): KeyTrait,
{
    fn get_main_key(key: &(MainKey, SubKey)) -> DbResult<Vec<u8>> {
        <CommonPrefix as DupSortUtils<(MainKey, SubKey), V>>::get_main_key(key)
    }

    fn get_sub_key(key: &(MainKey, SubKey)) -> DbResult<Vec<u8>> {
        <CommonPrefix as DupSortUtils<(MainKey, SubKey), V>>::get_sub_key(key)
    }

    fn get_sub_key_value<'a>(key: &(MainKey, SubKey), value: &V::Value) -> DbResult<Vec<u8>> {
        let mut res = Vec::with_capacity(SubKey::SIZE + V::SIZE);
        key.1.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        value.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        Ok(res)
    }

    fn get_first_key_after(key: &(MainKey, SubKey)) -> DbResult<Vec<u8>> {
        let total_size = SubKey::SIZE + V::SIZE;
        let mut res = Vec::with_capacity(total_size);
        key.1.serialize_into(&mut res).map_err(|_| DbError::Serialization)?;
        // Libmdbx requires that for DUP_FIXED tables, the value size will fixed even for getters
        // function, so the padding is a must.
        res.resize(total_size, 0);
        Ok(res)
    }

    fn next_main_key(main_key_bytes: &mut Vec<u8>) {
        <CommonPrefix as DupSortUtils<(MainKey, SubKey), V>>::next_main_key(main_key_bytes);
    }

    fn get_key_value_pair(
        main_key: &[u8],
        sub_key_value: &[u8],
    ) -> Option<((MainKey, SubKey), <V as ValueSerde>::Value)> {
        <CommonPrefix as DupSortUtils<(MainKey, SubKey), V>>::get_key_value_pair(
            main_key,
            sub_key_value,
        )
    }
}

// Adds one to the number represented by the bytes.
fn add_one(bytes: &mut Vec<u8>) {
    for byte in bytes.iter_mut().rev() {
        if *byte == u8::MAX {
            *byte = 0;
        } else {
            *byte += 1;
            return; // No need to continue if there is no carry.
        }
    }

    // If we reach this point, it means there was a carry into the most significant byte.
    bytes.insert(0, 1);
}

impl DbWriter {
    #[allow(dead_code)]
    pub(crate) fn create_common_prefix_table<
        MainKey: KeyTrait + Debug,
        SubKey: KeyTrait + Debug,
        V: ValueSerde + Debug,
    >(
        &mut self,
        name: &'static str,
    ) -> DbResult<TableIdentifier<(MainKey, SubKey), V, CommonPrefix>>
    where
        (MainKey, SubKey): KeyTrait + Debug,
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

    #[allow(dead_code)]
    pub(crate) fn create_common_prefix_fixed_size_table<
        K0: KeyTrait + Debug,
        K1: KeyTrait + Debug + ConstSerializationSize,
        V: ValueSerde + Debug + ConstSerializationSize,
    >(
        &mut self,
        name: &'static str,
    ) -> DbResult<TableIdentifier<(K0, K1), V, CommonPrefixFixedSize>>
    where
        (K0, K1): KeyTrait + Debug,
    {
        let txn = self.env.begin_rw_txn()?;
        txn.create_table(Some(name), TableFlags::DUP_SORT | TableFlags::DUP_FIXED)?;
        txn.commit()?;
        Ok(TableIdentifier {
            name,
            _key_type: PhantomData {},
            _value_type: PhantomData {},
            _table_type: PhantomData {},
        })
    }
}

impl<'env, K: KeyTrait + Debug, V: ValueSerde + Debug, T: DupSortTableType + DupSortUtils<K, V>>
    Table<'env> for TableHandle<'env, K, V, T>
{
    type Key = K;
    type Value = V;
    type TableVariant = T;

    fn cursor<'txn, Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, Self::Key, Self::Value, T>> {
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
        let main_key = T::get_main_key(key)?;
        let first_key = T::get_first_key_after(key)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        let Some(bytes) = cursor.get_both_range::<Cow<'_, [u8]>>(&main_key, &first_key)? else {
            return Ok(None);
        };

        let sub_key = T::get_sub_key(key)?;
        if let Some(mut bytes) = bytes.strip_prefix(sub_key.as_slice()) {
            let value = V::deserialize(&mut bytes).ok_or(DbError::InnerDeserialization)?;
            return Ok(Some(value));
        }
        Ok(None)
    }

    fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()> {
        let main_key = T::get_main_key(key)?;
        let sub_key = T::get_sub_key(key)?;
        let sub_key_value = T::get_sub_key_value(key, value)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        cursor.put(&main_key, &sub_key_value, WriteFlags::UPSERT)?;

        // TODO(dvir): consider return the cursor to the original position using prev instead of
        // cloning.
        let mut cloned_cursor = cursor.clone();
        if let Some((_key_prefix, sub_key_value)) =
            cloned_cursor.next_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if sub_key_value.starts_with(&sub_key) {
                cloned_cursor.del(WriteFlags::empty())?;
                return Ok(());
            }
        };

        if let Some((_key_prefix, sub_key_value)) =
            cursor.prev_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if sub_key_value.starts_with(&sub_key) {
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
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()> {
        let main_key = T::get_main_key(key)?;
        let sub_key = T::get_sub_key(key)?;
        let sub_key_value = T::get_sub_key_value(key, value)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        cursor.put(&main_key, &sub_key_value, WriteFlags::NO_DUP_DATA).map_err(
            |err| match err {
                libmdbx::Error::KeyExist => {
                    DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(self.name, key, value))
                }
                _ => err.into(),
            },
        )?;

        if let Some((_key_prefix, sub_key_value)) =
            cursor.clone().next_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if sub_key_value.starts_with(&sub_key) {
                cursor.del(WriteFlags::empty())?;
                return Err(DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(
                    self.name, key, value,
                )));
            }
        };

        if let Some((_key_prefix, sub_key_value)) =
            cursor.clone().prev_dup::<DbKeyType<'_>, DbValueType<'_>>()?
        {
            if sub_key_value.starts_with(&sub_key) {
                cursor.del(WriteFlags::empty())?;
                return Err(DbError::KeyAlreadyExists(KeyAlreadyExistsError::new(
                    self.name, key, value,
                )));
            }
        };
        Ok(())
    }

    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &Self::Key) -> DbResult<()> {
        let main_key = T::get_main_key(key)?;
        let first_key = T::get_first_key_after(key)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        let Some(bytes) = cursor.get_both_range::<Cow<'_, [u8]>>(&main_key, &first_key)? else {
            return Ok(());
        };

        let sub_key = T::get_sub_key(key)?;
        if bytes.starts_with(&sub_key) {
            cursor.del(WriteFlags::empty())?;
        }
        Ok(())
    }
}

#[allow(private_bounds)]
impl<'env, K: KeyTrait + Debug, V: ValueSerde + Debug, T: DupSortTableType + DupSortUtils<K, V>>
    TableHandle<'env, K, V, T>
{
    // Append a new value to the given key. The sub key must be the last for the given main key,
    // otherwise an error will be returned.
    #[allow(dead_code)]
    pub(crate) fn append(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &K,
        value: &<V as ValueSerde>::Value,
    ) -> DbResult<()> {
        let main_key = T::get_main_key(key)?;
        let sub_key_value = T::get_sub_key_value(key, value)?;

        let mut cursor = txn.txn.cursor(&self.database)?;
        cursor
            .put(&main_key, &sub_key_value, WriteFlags::APPEND_DUP)
            .map_err(|_| DbError::Append)?;
        if let Some(prev) = cursor.prev_dup::<DbKeyType<'_>, DbValueType<'_>>()? {
            if prev.1.starts_with(&T::get_sub_key(key)?) {
                cursor.next_dup::<DbKeyType<'_>, DbValueType<'_>>()?;
                cursor.del(WriteFlags::empty())?;
                return Err(DbError::Append);
            }
        }

        Ok(())
    }
}

impl<
    'txn,
    Mode: TransactionKind,
    K: KeyTrait + Debug,
    V: ValueSerde + Debug,
    T: DupSortTableType + DupSortUtils<K, V>,
> DbCursorTrait for DbCursor<'txn, Mode, K, V, T>
{
    type Key = K;
    type Value = V;

    fn prev(&mut self) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>> {
        let prev_cursor_res = self.cursor.prev::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((main_key_bytes, sub_key_value_bytes)) => {
                Ok(T::get_key_value_pair(&main_key_bytes, &sub_key_value_bytes))
            }
        }
    }

    fn next(&mut self) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>> {
        let prev_cursor_res = self.cursor.next::<DbKeyType<'_>, DbValueType<'_>>()?;
        match prev_cursor_res {
            None => Ok(None),
            Some((main_key_bytes, sub_key_value_bytes)) => {
                Ok(T::get_key_value_pair(&main_key_bytes, &sub_key_value_bytes))
            }
        }
    }

    // TODO(dvir): make this function walk only once on the table and not twice. This will
    // require to add functionality of libmdbx to the binding or use assumption on the current
    // implementation of the binding.
    fn lower_bound(
        &mut self,
        key: &Self::Key,
    ) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>> {
        let mut main_key = T::get_main_key(key)?;
        let first_key = T::get_first_key_after(key)?;

        // First try to find a match for the main key.
        if let Some(value_bytes) =
            self.cursor.get_both_range::<DbValueType<'_>>(&main_key, &first_key)?
        {
            return Ok(T::get_key_value_pair(&main_key, &value_bytes));
        }

        // The next main key bytes.
        T::next_main_key(&mut main_key);

        let Some((main_key_bytes, sub_key_value_bytes)) =
            self.cursor.set_range::<DbKeyType<'_>, DbValueType<'_>>(&main_key)?
        else {
            return Ok(None);
        };

        Ok(T::get_key_value_pair(&main_key_bytes, &sub_key_value_bytes))
    }
}
