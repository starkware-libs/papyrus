use std::fmt::Debug;
use std::marker::PhantomData;

use libmdbx::Cursor;

use super::serialization::{Key as KeyTrait, ValueSerde};
use super::{DbResult, DbTransaction, TransactionKind, RW};

mod dup_sort_tables;
mod simple_table;
#[allow(unused_imports)]
pub(crate) use dup_sort_tables::CommonPrefix;
pub(crate) use simple_table::SimpleTable;
#[cfg(test)]
pub(crate) mod test_utils;

// TODO(dvir): consider adding the create_table method to the Table trait.
// TODO(dvir): consider adding unchecked version of the those functions.

pub(crate) trait Table<'env> {
    type Key: KeyTrait + Debug;
    type Value: ValueSerde + Debug;
    type TableVariant: TableType;

    // TODO(dvir): consider move this to the cursor trait and get rid of the TableVariant type.
    // Create a cursor for the table.
    #[allow(clippy::type_complexity)]
    fn cursor<'txn, Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, Self::Key, Self::Value, Self::TableVariant>>;

    // Get a key value pair from the table.
    fn get<Mode: TransactionKind>(
        &'env self,
        txn: &'env DbTransaction<'env, Mode>,
        key: &Self::Key,
    ) -> DbResult<Option<<Self::Value as ValueSerde>::Value>>;

    // Insert or update a key value pair in the table. If the key already exists, the value will be
    // updated.
    fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()>;

    // Insert a key value pair in the table. If the key already exists, an error will be returned.
    fn insert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()>;

    // Append a key value pair to the end of the table. The key must be bigger than or equal to
    // the last key in the table; otherwise, an error will be returned.
    #[allow(dead_code)]
    fn append(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()>;

    // Delete a key value pair from the table.
    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &Self::Key) -> DbResult<()>;
}

// TODO(dvir): consider adding append functionality using a cursor. It should be more efficient for
// more than a single append operation (also for other table types).
pub(crate) trait DbCursorTrait {
    type Key: KeyTrait + Debug;
    type Value: ValueSerde + Debug;

    // Position at the previous key.
    #[allow(clippy::type_complexity)]
    fn prev(&mut self) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>>;

    // Position at the next key.
    #[allow(clippy::type_complexity)]
    fn next(&mut self) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>>;

    // Position at first key greater than or equal to specified key.
    #[allow(clippy::type_complexity)]
    fn lower_bound(
        &mut self,
        key: &Self::Key,
    ) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>>;
}

pub(crate) struct DbCursor<'txn, Mode: TransactionKind, K: KeyTrait, V: ValueSerde, T: TableType> {
    cursor: Cursor<'txn, Mode::Internal>,
    _key_type: PhantomData<K>,
    _value_type: PhantomData<V>,
    _table_type: PhantomData<T>,
}

pub(crate) trait TableType {}

// A value place holder for tables where we don't need a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct NoValue;
