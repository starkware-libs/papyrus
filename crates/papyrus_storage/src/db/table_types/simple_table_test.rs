use crate::db::table_types::test_utils::table_test;
use crate::db::DbWriter;

#[test]
fn simple_table_test() {
    table_test(DbWriter::create_simple_table);
}
