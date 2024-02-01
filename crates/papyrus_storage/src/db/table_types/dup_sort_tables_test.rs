use crate::db::table_types::dup_sort_tables::add_one;
use crate::db::table_types::test_utils::{random_table_test, table_cursor_test, table_test};
use crate::db::DbWriter;

#[test]
fn common_prefix_table() {
    table_test(DbWriter::create_common_prefix_table);
}

// Ignore because this test takes few seconds to run.
#[ignore]
#[test]
fn common_prefix_compare_with_simple_table_random_test() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let simple_table = writer.create_simple_table("simple_table").unwrap();
    let common_prefix_table = writer.create_common_prefix_table("common_prefix_table").unwrap();
    random_table_test(simple_table, common_prefix_table, &reader, &mut writer);
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
