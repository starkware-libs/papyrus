use assert_matches::assert_matches;
use rand::rngs::ThreadRng;
use rand::Rng;
use tracing::debug;

use super::{Table, TableType};
use crate::db::db_test::get_test_env;
use crate::db::serialization::{NoVersionValueWrapper, StorageSerde, StorageSerdeError};
use crate::db::table_types::{DbCursor, DbCursorTrait};
use crate::db::{DbReader, DbResult, DbWriter, TableHandle, TableIdentifier, RO, RW};
use crate::serialization::serializers::auto_storage_serde;
use crate::DbError;

// TODO(dvir): consider adding tests with keys and values in different sizes.

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

    let append_test_table = create_table(&mut writer, "append_test").unwrap();
    append_test(append_test_table, &mut writer);

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

fn append_test<T: TableType>(
    table_id: TableIdentifier<TableKey, TableValue, T>,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T>:
        Table<'env, Key = TableKey, Value = TableValue>,
{
    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();

    // Append to an empty table.
    table.append(&txn, &(1, 1), &11).unwrap();
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(11));

    // Successful appends.
    table.append(&txn, &(1, 1), &0).unwrap();
    table.append(&txn, &(1, 2), &12).unwrap();
    table.append(&txn, &(2, 0), &20).unwrap();
    table.append(&txn, &(2, 2), &22).unwrap();

    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(0));
    assert_eq!(table.get(&txn, &(1, 2)).unwrap(), Some(12));
    assert_eq!(table.get(&txn, &(2, 0)).unwrap(), Some(20));
    assert_eq!(table.get(&txn, &(2, 2)).unwrap(), Some(22));

    // Override the last key with a smaller value.
    table.append(&txn, &(2, 2), &0).unwrap();
    assert_eq!(table.get(&txn, &(2, 2)).unwrap(), Some(0));

    // Override the last key with a bigger value.
    table.append(&txn, &(2, 2), &100).unwrap();
    assert_eq!(table.get(&txn, &(2, 2)).unwrap(), Some(100));

    // Append key that is not the last, should fail.
    assert_matches!(table.append(&txn, &(0, 0), &0), Err(DbError::Append));
    assert_matches!(table.append(&txn, &(1, 3), &0), Err(DbError::Append));
    assert_matches!(table.append(&txn, &(2, 1), &0), Err(DbError::Append));

    // Check the final database.
    assert_eq!(table.get(&txn, &(1, 1)).unwrap(), Some(0));
    assert_eq!(table.get(&txn, &(1, 2)).unwrap(), Some(12));
    assert_eq!(table.get(&txn, &(2, 0)).unwrap(), Some(20));
    assert_eq!(table.get(&txn, &(2, 2)).unwrap(), Some(100));
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

// Constants for random_table_test.
// Use 300 as the maximum value to make the values spread over more than one byte but small enough
// to make collisions.
const MAX_VALUE: u32 = 300;
const MAIN_KEY_MAX_VALUE: u32 = 300;
const SUB_KEY_MAX_VALUE: u32 = 300;
// Number of iterations to run the test.
const ITERS: usize = 1000;
// Number of get calls to make in each iteration.
const GET_CALLS: usize = 10;
// Number of cursor test iterations to make in each main iteration.
const CURSOR_ITERS: usize = 2;
// Number of cursor operations to make in each cursor iteration.
const CURSOR_OPS_NUM: usize = 7;

pub(crate) fn random_table_test<T0: TableType, T1: TableType>(
    first_table_id: TableIdentifier<TableKey, TableValue, T0>,
    second_table_id: TableIdentifier<TableKey, TableValue, T1>,
    reader: &DbReader,
    writer: &mut DbWriter,
) where
    for<'env> TableHandle<'env, TableKey, TableValue, T0>:
        Table<'env, Key = TableKey, Value = TableValue, TableVariant = T0>,
    for<'env> TableHandle<'env, TableKey, TableValue, T1>:
        Table<'env, Key = TableKey, Value = TableValue, TableVariant = T1>,
    for<'txn> DbCursor<'txn, RO, TableKey, TableValue, T0>:
        DbCursorTrait<Key = TableKey, Value = TableValue>,
    for<'txn> DbCursor<'txn, RO, TableKey, TableValue, T1>:
        DbCursorTrait<Key = TableKey, Value = TableValue>,
{
    let _ = simple_logger::init_with_env();
    let rtxn = reader.begin_ro_txn().unwrap();
    let first_table = rtxn.open_table(&first_table_id).unwrap();
    let second_table = rtxn.open_table(&second_table_id).unwrap();
    let mut rng = rand::thread_rng();

    for iter in 0..ITERS {
        debug!("iteration: {iter:?}");
        let wtxn = writer.begin_rw_txn().unwrap();
        let random_op = rng.gen_range(0..4);
        let key = get_random_key(&mut rng);
        let value = rng.gen_range(0..MAX_VALUE);

        // Insert, upsert, or delete a random key.
        if random_op == 0 {
            // Insert
            debug!("insert: {key:?}, {value:?}");
            let first_res = first_table.insert(&wtxn, &key, &value);
            let second_res = second_table.insert(&wtxn, &key, &value);
            assert!(
                (first_res.is_ok() && second_res.is_ok())
                    || (matches!(first_res.unwrap_err(), DbError::KeyAlreadyExists(..))
                        && matches!(second_res.unwrap_err(), DbError::KeyAlreadyExists(..)))
            );
        } else if random_op == 1 {
            // Upsert
            debug!("upsert: {key:?}, {value:?}");
            first_table.upsert(&wtxn, &key, &value).unwrap();
            second_table.upsert(&wtxn, &key, &value).unwrap();
        } else if random_op == 2 {
            // Append
            // TODO(dvir): consider increasing the number of successful appends (append of not the
            // last entry will fail).
            debug!("append: {key:?}, {value:?}");
            let first_res = first_table.append(&wtxn, &key, &value);
            let second_res = second_table.append(&wtxn, &key, &value);
            assert!(
                (first_res.is_ok() && second_res.is_ok())
                    || (matches!(first_res.unwrap_err(), DbError::Append)
                        && matches!(second_res.unwrap_err(), DbError::Append))
            );
        } else if random_op == 3 {
            // Delete
            debug!("delete: {key:?}");
            first_table.delete(&wtxn, &key).unwrap();
            second_table.delete(&wtxn, &key).unwrap();
        }

        wtxn.commit().unwrap();
        let rtxn = reader.begin_ro_txn().unwrap();

        // Compare get calls.
        let mut keys_list = vec![key];
        for _ in 0..GET_CALLS {
            keys_list.push(get_random_key(&mut rng));
        }

        for key in keys_list {
            let first_value = first_table.get(&rtxn, &key).unwrap();
            let second_value = second_table.get(&rtxn, &key).unwrap();
            assert_eq!(
                first_value, second_value,
                "Mismatch for key {key:?}\n first key: {first_value:?}\n second key: \
                 {second_value:?}"
            );
        }

        // Compare cursor calls.
        let mut keys_list = vec![key];
        for _ in 0..CURSOR_ITERS {
            keys_list.push(get_random_key(&mut rng));
        }

        for key in keys_list {
            debug!("lower_bound: {key:?}");
            let mut first_cursor = first_table.cursor(&rtxn).unwrap();
            let first_res = first_cursor.lower_bound(&key).unwrap();
            let mut second_cursor = second_table.cursor(&rtxn).unwrap();
            let second_res = second_cursor.lower_bound(&key).unwrap();
            assert_eq!(
                first_res, second_res,
                "Mismatch for key {key:?}\n first key: {first_res:?}\n second key: {second_res:?}"
            );

            for _ in 0..CURSOR_OPS_NUM {
                let random_op = rng.gen_range(0..2);
                if random_op == 0 {
                    // Next
                    debug!("next: {key:?}");
                    let first_res = first_cursor.next().unwrap();
                    let second_res = second_cursor.next().unwrap();
                    assert_eq!(
                        first_res, second_res,
                        "Mismatch for key {key:?}\n first key: {first_res:?}\n second key: \
                         {second_res:?}"
                    );
                } else if random_op == 1 {
                    // Prev
                    debug!("prev: {key:?}");
                    let first_res = first_cursor.prev().unwrap();
                    let second_res = second_cursor.prev().unwrap();
                    assert_eq!(
                        first_res, second_res,
                        "Mismatch for key {key:?}\n first key: {first_res:?}\n second key: \
                         {second_res:?}"
                    );
                }
            }
        }
    }
}

fn get_random_key(rng: &mut ThreadRng) -> TableKey {
    (rng.gen_range(0..MAIN_KEY_MAX_VALUE), rng.gen_range(0..SUB_KEY_MAX_VALUE))
}
