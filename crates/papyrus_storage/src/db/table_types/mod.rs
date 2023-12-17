use std::fmt::Debug;

use super::serialization::StorageSerde;
use super::{DbCursor, DbResult, DbTransaction, TransactionKind, RW};

pub(crate) mod simple_table;
#[cfg(test)]
pub(crate) mod test_utils;

pub(crate) trait Table<'env> {
    type Key: StorageSerde + Debug;
    type Value: StorageSerde + Debug;
    type TableType;

    #[allow(clippy::type_complexity)]
    fn cursor<'txn, Mode: TransactionKind>(
        &'env self,
        txn: &'txn DbTransaction<'env, Mode>,
    ) -> DbResult<DbCursor<'txn, Mode, Self::Key, Self::Value, Self::TableType>>;

    fn get<Mode: TransactionKind>(
        &'env self,
        txn: &'env DbTransaction<'env, Mode>,
        key: &Self::Key,
    ) -> DbResult<Option<Self::Value>>;

    fn upsert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &Self::Value,
    ) -> DbResult<()>;

    fn insert(
        &'env self,
        txn: &DbTransaction<'env, RW>,
        key: &Self::Key,
        value: &Self::Value,
    ) -> DbResult<()>;

    fn delete(&'env self, txn: &DbTransaction<'env, RW>, key: &Self::Key) -> DbResult<()>;
}
