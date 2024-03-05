use crate::db::db_test::get_test_env;
use crate::db::table_types::dup_sort_tables::add_one;
use crate::db::table_types::test_utils::{table_cursor_test, table_test};

#[test]
fn common_prefix_table() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_common_prefix_table("table").unwrap();
    table_test(table_id, &reader, &mut writer);
}

#[test]
fn common_prefix_table_cursor() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_common_prefix_table("table").unwrap();
    table_cursor_test(table_id, &reader, &mut writer);
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
