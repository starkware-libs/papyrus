use super::{Table, TableType};
use crate::db::db_test::get_test_env;
use crate::db::serialization::{NoVersionValueWrapper, StorageSerde, StorageSerdeError};
use crate::db::table_types::{DbCursor, DbCursorTrait};
use crate::db::{DbResult, DbWriter, TableHandle, TableIdentifier, RW};
use crate::serialization::serializers::auto_storage_serde;

pub(crate) type TableKey = (u32, u32);
pub(crate) type TableValue = NoVersionValueWrapper<u32>;

auto_storage_serde! {
    (u32, u32);
}

// A generic test for all table types.
#[allow(clippy::type_complexity)]
pub(crate) fn table_test<T: TableType>(
    create_table: fn(
        &mut DbWriter,
        &'static str,
    ) -> DbResult<TableIdentifier<TableKey, TableValue, T>>,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue, TableVariant = T>,
    for<'txn> DbCursor<'txn, RW, TableKey, TableValue, T>:
        DbCursorTrait<Key = TableKey, Value = TableValue>,
{
    let ((_reader, mut writer), _temp_dir) = get_test_env();

    let get_test_table = create_table(&mut writer, "get_test").unwrap();
    get_test(get_test_table, &mut writer);

    let insert_test_table = create_table(&mut writer, "insert_test").unwrap();
    insert_test(insert_test_table, &mut writer);

    let upsert_test_table = create_table(&mut writer, "upsert_test").unwrap();
    upsert_test(upsert_test_table, &mut writer);

    let delete_test_table = create_table(&mut writer, "delete_test").unwrap();
    delete_test(delete_test_table, &mut writer);

    let cursor_test_table = create_table(&mut writer, "cursor_test").unwrap();
    table_cursor_test(cursor_test_table, &mut writer);
}

fn get_test<T: TableType>(table_id: TableIdentifier<TableKey, TableValue, T>, writer: &mut DbWriter)
where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue>,
{
    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();

    // Read does not exist value.
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), None);

    // Insert and read a value.
    table.insert(&txn, &(1, 1), &11).unwrap();
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(11));
}

fn insert_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue>,
{
    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();

    // Insert values.
    table.insert(&txn, &(1, 2), &12).unwrap();
    table.insert(&txn, &(2, 1), &21).unwrap();
    table.insert(&txn, &(1, 1), &11).unwrap();

    assert_eq!(table.get(&txn, &(1, 2)).unwrap(), Some(12));
    assert_eq!(table.get(&txn, &(2, 1)).unwrap(), Some(21));
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(11));

    // Insert duplicate key.
    assert_eq!(
        table.insert(&txn, &(1, 1), &0).expect_err("Expected KeyAlreadyExistsError").to_string(),
        format!(
            "Key '{key:?}' already exists in table '{table_name}'. Error when tried to insert \
             value '0'",
            key = (1, 1),
            table_name = table_id.name
        ),
    );

    // Check the final database.
    assert_eq!(table.get(&txn, &(1, 2)).unwrap(), Some(12));
    assert_eq!(table.get(&txn, &(2, 1)).unwrap(), Some(21));
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(11));
}

fn upsert_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue>,
{
    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();

    // Upsert not existing key.
    table.upsert(&txn, &(1, 1), &11).unwrap();
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(11));

    // (1,1) was already inserted, so this is an update.
    table.upsert(&txn, &(1, 1), &0).unwrap();
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(0));
}

fn delete_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue>,
{
    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();

    table.insert(&txn, &(1, 1), &11).unwrap();
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(11));

    table.delete(&txn, &(1, 1)).unwrap();
    // Delete non-existent value.
    table.delete(&txn, &(2, 2)).unwrap();

    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), None);
    assert_eq!(table.get(&txn, &(2, 2)).unwrap(), None);
}

fn table_cursor_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue, TableVariant = T>,
    for<'txn> DbCursor<'txn, RW, TableKey, TableValue, T>:
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

    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();

    // Insert the values to the table.
    for (k, v) in &VALUES {
        table.insert(&txn, k, v).unwrap();
    }

    // Test lower_bound().
    let mut cursor = table.cursor(&txn).unwrap();
    let current = cursor.lower_bound(&(0, 0)).unwrap();
    assert_eq!(current, Some(((1, 1), 7)));
    let current = cursor.lower_bound(&(2, 2)).unwrap();
    assert_eq!(current, Some(((2, 2), 2)));
    let current = cursor.lower_bound(&(2, 4)).unwrap();
    assert_eq!(current, Some(((3, 1), 8)));
    let current = cursor.lower_bound(&(4, 4)).unwrap();
    assert_eq!(current, None);

    // Iterate using next().
    let mut cursor = table.cursor(&txn).unwrap();
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
    let mut cursor = table.cursor(&txn).unwrap();
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
