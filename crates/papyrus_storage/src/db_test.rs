use super::{open_env, DbReader, DbWriter};
use crate::test_utils::get_test_config;

fn get_test_env() -> (DbReader, DbWriter) {
    let config = get_test_config();
    open_env(config).expect("Failed to open environment.")
}

#[test]
fn test_open_env() {
    get_test_env();
}

#[test]
fn test_txns() {
    // Create an environment and a table.
    let (reader, mut writer) = get_test_env();
    let table_id = writer.create_table::<[u8; 3], [u8; 5]>("table").unwrap();

    // Snapshot state by creating a read txn.
    let txn0 = reader.begin_ro_txn().unwrap();
    let table = txn0.open_table(&table_id).unwrap();

    // Insert a value.
    let wtxn = writer.begin_rw_txn().unwrap();
    table.insert(&wtxn, b"key", b"data0").unwrap();
    wtxn.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn1 = reader.begin_ro_txn().unwrap();

    // Update the value.
    let wtxn = writer.begin_rw_txn().unwrap();
    table.upsert(&wtxn, b"key", b"data1").unwrap();
    wtxn.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn2 = reader.begin_ro_txn().unwrap();

    // Validate data in snapshots.
    assert_eq!(table.get(&txn0, b"key").unwrap(), None);
    assert_eq!(table.get(&txn1, b"key").unwrap(), Some(*b"data0"));
    assert_eq!(table.get(&txn2, b"key").unwrap(), Some(*b"data1"));
}
#[test]

fn test_stats() {
    // Create an environment and a table.
    let (reader, mut writer) = get_test_env();
    let table_id = writer.create_table::<[u8; 3], [u8; 5]>("table").unwrap();

    // Empty table stats.
    let empty_stat = reader.get_stats("table").unwrap();
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 0);
    assert_eq!(empty_stat.entries, 0);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 0);

    // Insert a value.
    let wtxn = writer.begin_rw_txn().unwrap();
    let table = wtxn.open_table(&table_id).unwrap();
    table.insert(&wtxn, b"key", b"data0").unwrap();
    wtxn.commit().unwrap();

    // Non-empty table stats.
    let empty_stat = reader.get_stats("table").unwrap();
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 1);
    assert_eq!(empty_stat.entries, 1);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 1);
}
