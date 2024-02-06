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
pub(crate) mod const_serialization_size;
#[cfg(test)]
pub(crate) mod test_utils;

pub(crate) trait Table<'env> {
    type Key: KeyTrait + Debug;
    type Value: ValueSerde + Debug;
    type TableVariant: TableType;

    // TODO(dvir): consider move this to the cursor trait and get rid of the TableVariant type.
    #[allow(clippy::type_complexity)]
    fn cursor<'txn, Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, Self::Key, Self::Value, Self::TableVariant>>;

    fn get<Mode: TransactionKind>(
        &'env self,
        txn: &'env DbTransaction<'env, Mode>,
        key: &Self::Key,
    ) -> DbResult<Option<<Self::Value as ValueSerde>::Value>>;

    fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()>;

    fn insert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &<Self::Value as ValueSerde>::Value,
    ) -> DbResult<()>;

    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &Self::Key) -> DbResult<()>;
}

pub(crate) trait DbCursorTrait {
    type Key: KeyTrait + Debug;
    type Value: ValueSerde + Debug;

    #[allow(clippy::type_complexity)]
    fn prev(&mut self) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>>;
    #[allow(clippy::type_complexity)]
    fn next(&mut self) -> DbResult<Option<(Self::Key, <Self::Value as ValueSerde>::Value)>>;
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
