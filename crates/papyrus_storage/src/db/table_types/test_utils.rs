use super::{Table, TableType};
use crate::db::serialization::{NoVersionValueWrapper, StorageSerde, StorageSerdeError};
use crate::db::table_types::{DbCursor, DbCursorTrait};
use crate::db::{DbReader, DbWriter, TableHandle, TableIdentifier, RO};
use crate::serialization::serializers::auto_storage_serde;

type TableKey = (u32, u32);
type TableValue = NoVersionValueWrapper<u32>;

auto_storage_serde! {
    (u32, u32);
}

pub(crate) fn table_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    reader: &DbReader,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue>,
{
    let rtxn = reader.begin_ro_txn().unwrap();
    let table = rtxn.open_table(&table_id).unwrap();

    // Read does not exist value.
    let rtxn = reader.begin_ro_txn().unwrap();
    assert_eq!(table.get(&rtxn, &(1, 1)).unwrap(), None);

    // Insert values.
    let wtxn = writer.begin_rw_txn().unwrap();
    table.insert(&wtxn, &(1, 2), &12).unwrap();
    table.insert(&wtxn, &(2, 1), &21).unwrap();
    table.insert(&wtxn, &(1, 1), &11).unwrap();
    wtxn.commit().unwrap();
    let rtxn = reader.begin_ro_txn().unwrap();
    assert_eq!(table.get(&rtxn, &(1, 2)).unwrap(), Some(12));
    assert_eq!(table.get(&rtxn, &(2, 1)).unwrap(), Some(21));
    assert_eq!(table.get(&rtxn, &(1, 1)).unwrap(), Some(11));

    // Insert duplicate key.
    let wtxn = writer.begin_rw_txn().unwrap();
    assert_eq!(
        table.insert(&wtxn, &(1, 1), &0).expect_err("Expected KeyAlreadyExistsError").to_string(),
        format!(
            "Key '{key:?}' already exists in table '{table_name}'. Error when tried to insert \
             value '0'",
            key = (1, 1),
            table_name = table_id.name
        ),
    );
    drop(wtxn);

    // Upsert a values.
    let wtxn = writer.begin_rw_txn().unwrap();
    // (1,1) was already inserted, so this is an update.
    table.upsert(&wtxn, &(1, 1), &0).unwrap();
    // (3,3) was not inserted, so this is an insert.
    table.upsert(&wtxn, &(3, 3), &33).unwrap();
    wtxn.commit().unwrap();
    let rtxn = reader.begin_ro_txn().unwrap();
    assert_eq!(table.get(&rtxn, &(1, 1)).unwrap(), Some(0));
    assert_eq!(table.get(&rtxn, &(3, 3)).unwrap(), Some(33));

    // Delete values.
    let wtxn = writer.begin_rw_txn().unwrap();
    table.delete(&wtxn, &(1, 1)).unwrap();
    // Delete non-existent value.
    table.delete(&wtxn, &(4, 4)).unwrap();
    wtxn.commit().unwrap();
    let rtxn = reader.begin_ro_txn().unwrap();
    assert_eq!(table.get(&rtxn, &(1, 1)).unwrap(), None);
    assert_eq!(table.get(&rtxn, &(4, 4)).unwrap(), None);
}

pub(crate) fn table_cursor_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    reader: &DbReader,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue, TableVariant = T>,
    for<'txn> DbCursor<'txn, RO, TableKey, TableValue, T>:
        DbCursorTrait<Key = TableKey, Value = TableValue>,
{
    const VALUES: [((u32, u32), u32); 9] = [
        ((2, 2), 2),
        ((1, 1), 7),
        ((3, 3), 4),
        ((3, 1), 8),
        ((1, 2), 9),
        ((2, 3), 5),
        ((1, 3), 1),
        ((3, 2), 1),
        ((2, 1), 5),
    ];

    const SORTED_VALUES: [((u32, u32), u32); 9] = [
        ((1, 1), 7),
        ((1, 2), 9),
        ((1, 3), 1),
        ((2, 1), 5),
        ((2, 2), 2),
        ((2, 3), 5),
        ((3, 1), 8),
        ((3, 2), 1),
        ((3, 3), 4),
    ];

    let rtxn = reader.begin_ro_txn().unwrap();
    let table = rtxn.open_table(&table_id).unwrap();

    // Insert the values to the table.
    let wtxn = writer.begin_rw_txn().unwrap();
    for (k, v) in &VALUES {
        table.insert(&wtxn, k, v).unwrap();
    }
    wtxn.commit().unwrap();

    // Test lower_bound().
    let rtxn = reader.begin_ro_txn().unwrap();
    let mut cursor = table.cursor(&rtxn).unwrap();
    let current = cursor.lower_bound(&(0, 0)).unwrap();
    assert_eq!(current, Some(((1, 1), 7)));
    let current = cursor.lower_bound(&(2, 2)).unwrap();
    assert_eq!(current, Some(((2, 2), 2)));
    let current = cursor.lower_bound(&(2, 4)).unwrap();
    assert_eq!(current, Some(((3, 1), 8)));
    let current = cursor.lower_bound(&(4, 4)).unwrap();
    assert_eq!(current, None);

    // Iterate using next().
    let rtxn = reader.begin_ro_txn().unwrap();
    let mut cursor = table.cursor(&rtxn).unwrap();
    let mut current = cursor.lower_bound(&(0, 0)).unwrap();
    for kv_pair in SORTED_VALUES {
        assert_eq!(current, Some(kv_pair));
        current = cursor.next().unwrap();
    }
    current = cursor.next().unwrap();
    assert_eq!(current, None);
    // In the end still return None.
    current = cursor.next().unwrap();
    assert_eq!(current, None);

    // Iterate using prev().
    let rtxn = reader.begin_ro_txn().unwrap();
    let mut cursor = table.cursor(&rtxn).unwrap();
    let mut current = cursor.lower_bound(&(4, 4)).unwrap();
    assert_eq!(current, None);
    for kv_pair in SORTED_VALUES.iter().rev().cloned() {
        current = cursor.prev().unwrap();
        assert_eq!(current, Some(kv_pair));
    }
    current = cursor.prev().unwrap();
    assert_eq!(current, None);
    // In the end still return None.
    current = cursor.prev().unwrap();
    assert_eq!(current, None);
}
