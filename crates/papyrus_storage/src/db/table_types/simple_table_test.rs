use assert_matches::assert_matches;

use crate::db::db_test::get_test_env;
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::test_utils::{table_cursor_test, table_test};
use crate::db::table_types::Table;
use crate::db::DbError;

#[test]
fn simple_table_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_simple_table("table").unwrap();
    table_test(table_id, &reader, &mut writer);
}

#[test]
fn simple_table_cursor_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_simple_table("table").unwrap();
    table_cursor_test(table_id, &reader, &mut writer);
}

#[test]
fn append_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_simple_table::<u32, NoVersionValueWrapper<u32>>("table").unwrap();

    let wtxn = writer.begin_rw_txn().unwrap();
    let handle = wtxn.open_table(&table_id).unwrap();
    handle.append(&wtxn, &1, &1).unwrap();
    handle.append(&wtxn, &2, &1).unwrap();
    handle.append(&wtxn, &2, &2).unwrap();
    handle.append(&wtxn, &3, &0).unwrap();

    let result = handle.append(&wtxn, &1, &2);
    assert_matches!(result, Err(DbError::Append));

    let result = handle.append(&wtxn, &1, &0);
    assert_matches!(result, Err(DbError::Append));

    wtxn.commit().unwrap();

    let rtxn = reader.begin_ro_txn().unwrap();
    let handle = rtxn.open_table(&table_id).unwrap();
    assert_eq!(handle.get(&rtxn, &1).unwrap(), Some(1));
    assert_eq!(handle.get(&rtxn, &2).unwrap(), Some(2));
    assert_eq!(handle.get(&rtxn, &3).unwrap(), Some(0));
}
