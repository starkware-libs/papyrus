use crate::db::table_types::dup_sort_tables::add_one;
use crate::db::table_types::test_utils::table_test;
use crate::db::DbWriter;

#[test]
fn common_prefix_table() {
    table_test(DbWriter::create_common_prefix_table);
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
