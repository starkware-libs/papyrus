use assert_matches::assert_matches;

use super::{DupSortTableType, DupSortUtils};
use crate::db::db_test::get_test_env;
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::dup_sort_tables::add_one;
use crate::db::table_types::test_utils::{random_table_test, table_cursor_test, table_test};
use crate::db::table_types::Table;
use crate::db::{DbError, DbReader, DbWriter, TableIdentifier};

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

// Ignore because this test takes few seconds to run.
#[ignore]
#[test]
fn common_prefix_compare_with_simple_table_random_test() {
    for _ in 0..5 {
        let ((reader, mut writer), _temp_dir) = get_test_env();
        let simple_table = writer.create_simple_table("simple_table").unwrap();
        let common_prefix_table = writer.create_common_prefix_table("common_prefix_table").unwrap();
        random_table_test(simple_table, common_prefix_table, &reader, &mut writer);
    }
}

#[test]
fn common_prefix_fixed_size_table_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_common_prefix_fixed_size_table("table").unwrap();
    table_test(table_id, &reader, &mut writer);
}

#[test]
fn common_prefix_table_fixed_size_cursor_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_common_prefix_fixed_size_table("table").unwrap();
    table_cursor_test(table_id, &reader, &mut writer);
}

#[ignore]
#[test]
fn common_prefix_fixed_size_compare_with_simple_table_random_test() {
    for _ in 0..5 {
        let ((reader, mut writer), _temp_dir) = get_test_env();
        let simple_table = writer.create_simple_table("simple_table").unwrap();
        let common_prefix_table =
            writer.create_common_prefix_fixed_size_table("common_prefix_fixed_size_table").unwrap();
        random_table_test(simple_table, common_prefix_table, &reader, &mut writer);
    }
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

#[test]
fn common_prefix_append_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_common_prefix_table("table").unwrap();
    dup_sort_append_test(table_id, &reader, &mut writer);
}

#[test]
fn common_prefix_fixed_size_append_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_common_prefix_fixed_size_table("table").unwrap();
    dup_sort_append_test(table_id, &reader, &mut writer);
}

fn dup_sort_append_test<T: DupSortTableType>(
    table_id: TableIdentifier<(u32, u32), NoVersionValueWrapper<u32>, T>,
    reader: &DbReader,
    writer: &mut DbWriter,
) where
    T: DupSortUtils<(u32, u32), NoVersionValueWrapper<u32>>,
{
    let wtxn = writer.begin_rw_txn().unwrap();
    let handle = wtxn.open_table(&table_id).unwrap();
    handle.append(&wtxn, &(2, 2), &1).unwrap();
    handle.append(&wtxn, &(2, 3), &2).unwrap();
    handle.append(&wtxn, &(1, 1), &3).unwrap();
    handle.append(&wtxn, &(3, 0), &4).unwrap();

    let result = handle.append(&wtxn, &(2, 2), &5);
    assert_matches!(result, Err(DbError::Append));

    // For DupSort tables append with key that already exists should fail.
    let result = handle.append(&wtxn, &(2, 3), &0);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append(&wtxn, &(2, 3), &5);
    assert_matches!(result, Err(DbError::Append));

    wtxn.commit().unwrap();

    let rtxn = reader.begin_ro_txn().unwrap();
    let handle = rtxn.open_table(&table_id).unwrap();
    assert_eq!(handle.get(&rtxn, &(2, 2)).unwrap(), Some(1));
    assert_eq!(handle.get(&rtxn, &(2, 3)).unwrap(), Some(2));
    assert_eq!(handle.get(&rtxn, &(1, 1)).unwrap(), Some(3));
    assert_eq!(handle.get(&rtxn, &(3, 0)).unwrap(), Some(4));
}
