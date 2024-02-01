use rand::rngs::ThreadRng;
use rand::Rng;
use tracing::debug;

use super::{Table, TableType};
use crate::db::serialization::{NoVersionValueWrapper, StorageSerde, StorageSerdeError};
use crate::db::table_types::{DbCursor, DbCursorTrait};
use crate::db::{DbError, DbReader, DbWriter, TableHandle, TableIdentifier, RO};
use crate::serializers::auto_storage_serde;

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

// Constants for random_table_test.
const MAX_VALUE: u32 = 20;
const MAIN_KEY_MAX_VALUE: u32 = 5;
const SUB_KEY_MAX_VALUE: u32 = 5;
// Number of iterations to run the test.
const ITERS: usize = 500;
// Number of get calls to make in each iteration.
const GET_CALLS: usize = 10;
// Number of cursor test iterations to make in each main iteration.
const CURSOR_ITERS: usize = 2;
// Number of cursor operations to make in each cursor iteration.
const CURSOR_OPS_NUM: usize = 4;

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
        let random_op = rng.gen_range(0..3);
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
