use assert_matches::assert_matches;

use super::{DupSortTableType, DupSortUtils};
use crate::db::db_test::get_test_env;
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::dup_sort_tables::add_one;
use crate::db::table_types::test_utils::{random_table_test, table_test, TableKey, TableValue};
use crate::db::table_types::Table;
use crate::db::{DbError, DbResult, DbWriter, TableIdentifier};

#[test]
fn common_prefix_table() {
    table_test(DbWriter::create_common_prefix_table);
}

// Ignore because this test takes few seconds to run.
#[ignore]
#[test]
fn common_prefix_compare_with_simple_table_random() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let simple_table = writer.create_simple_table("simple_table").unwrap();
    let common_prefix_table = writer.create_common_prefix_table("common_prefix_table").unwrap();
    random_table_test(simple_table, common_prefix_table, &reader, &mut writer);
}

#[test]
fn common_prefix_append_greater_sub_key() {
    append_greater_sub_key_test(DbWriter::create_common_prefix_table);
}

#[allow(clippy::type_complexity)]
fn append_greater_sub_key_test<T>(
    create_table: fn(
        &mut DbWriter,
        &'static str,
    ) -> DbResult<TableIdentifier<TableKey, TableValue, T>>,
) where
    T: DupSortTableType + DupSortUtils<(u32, u32), NoVersionValueWrapper<u32>>,
{
    let ((_reader, mut writer), _temp_dir) = get_test_env();
    let table_id = create_table(&mut writer, "table").unwrap();

    let txn = writer.begin_rw_txn().unwrap();

    let handle = txn.open_table(&table_id).unwrap();
    handle.append_greater_sub_key(&txn, &(2, 2), &22).unwrap();
    handle.append_greater_sub_key(&txn, &(2, 3), &23).unwrap();
    handle.append_greater_sub_key(&txn, &(1, 1), &11).unwrap();
    handle.append_greater_sub_key(&txn, &(3, 0), &30).unwrap();

    // For DupSort tables append with key that already exists should fail. Try append with smaller
    // bigger and equal values.
    let result = handle.append_greater_sub_key(&txn, &(2, 2), &0);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(2, 2), &22);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(2, 2), &100);
    assert_matches!(result, Err(DbError::Append));

    // As before, but for the last main key.
    let result = handle.append_greater_sub_key(&txn, &(3, 0), &0);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(3, 0), &30);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append_greater_sub_key(&txn, &(3, 0), &100);
    assert_matches!(result, Err(DbError::Append));

    // Check the final database.
    assert_eq!(handle.get(&txn, &(2, 2)).unwrap(), Some(22));
    assert_eq!(handle.get(&txn, &(2, 3)).unwrap(), Some(23));
    assert_eq!(handle.get(&txn, &(1, 1)).unwrap(), Some(11));
    assert_eq!(handle.get(&txn, &(3, 0)).unwrap(), Some(30));
}

#[test]
fn add_one_test() {
    let mut bytes;

    bytes = vec![0];
    add_one(&mut bytes);
    assert_eq!(bytes, vec![1]);

    bytes = vec![u8::MAX];
    add_one(&mut bytes);
    assert_eq!(bytes, vec![1, 0]);

    bytes = vec![1, 0, u8::MAX];
    add_one(&mut bytes);
    assert_eq!(bytes, vec![1, 1, 0]);
}
