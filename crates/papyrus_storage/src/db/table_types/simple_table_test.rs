use crate::db::db_test::get_test_env;
use crate::db::table_types::test_utils::{table_cursor_test, table_test};

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
