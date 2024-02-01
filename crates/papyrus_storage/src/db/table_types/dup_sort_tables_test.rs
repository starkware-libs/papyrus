use crate::db::db_test::get_test_env;
use crate::db::table_types::test_utils::{random_table_test, table_cursor_test, table_test};

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
