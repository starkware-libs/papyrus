use libmdbx::PageSize;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use crate::db::{get_page_size, open_env, DbReader, DbWriter};
use crate::test_utils::get_test_config;

fn get_test_env() -> ((DbReader, DbWriter), TempDir) {
    let (config, temp_dir) = get_test_config();
    (open_env(config).expect("Failed to open environment."), temp_dir)
}

#[test]
fn open_env_scenario() {
    get_test_env();
}

#[test]
fn txns_scenarios() {
    // Create an environment and a table.
    let ((reader, mut writer), _temp_dir) = get_test_env();
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

    // Delete the value.
    let wtxn2 = writer.begin_rw_txn().unwrap();
    table.delete(&wtxn2, b"key").unwrap();
    wtxn2.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn3 = reader.begin_ro_txn().unwrap();

    // Validate data in snapshots.
    assert_eq!(table.get(&txn0, b"key").unwrap(), None);
    assert_eq!(table.get(&txn1, b"key").unwrap(), Some(*b"data0"));
    assert_eq!(table.get(&txn2, b"key").unwrap(), Some(*b"data1"));
    assert_eq!(table.get(&txn3, b"key").unwrap(), None);
}
#[test]

fn table_stats() {
    // Create an environment and a table.
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id = writer.create_table::<[u8; 3], [u8; 5]>("table").unwrap();

    // Empty table stats.
    let empty_stat = reader.get_table_stats("table").unwrap();
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
    let empty_stat = reader.get_table_stats("table").unwrap();
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 1);
    assert_eq!(empty_stat.entries, 1);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 1);

    // Delete the value.
    let wtxn = writer.begin_rw_txn().unwrap();
    let table = wtxn.open_table(&table_id).unwrap();
    table.delete(&wtxn, b"key").unwrap();
    wtxn.commit().unwrap();

    // Empty table stats.
    let empty_stat = reader.get_table_stats("table").unwrap();
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 0);
    assert_eq!(empty_stat.entries, 0);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 0);
}

use super::{MDBX_MAX_PAGESIZE, MDBX_MIN_PAGESIZE};
#[test]
fn get_page_size_test() {
    // Good values.
    assert_eq!(get_page_size(MDBX_MIN_PAGESIZE), PageSize::Set(MDBX_MIN_PAGESIZE));
    assert_eq!(get_page_size(4096), PageSize::Set(4096));
    assert_eq!(get_page_size(MDBX_MAX_PAGESIZE), PageSize::Set(MDBX_MAX_PAGESIZE));

    // Range fix.
    assert_eq!(get_page_size(MDBX_MIN_PAGESIZE - 1), PageSize::Set(MDBX_MIN_PAGESIZE));
    assert_eq!(get_page_size(MDBX_MAX_PAGESIZE + 1), PageSize::Set(MDBX_MAX_PAGESIZE));

    // Power of two fix.
    assert_eq!(get_page_size(1025), PageSize::Set(1024));
    assert_eq!(get_page_size(2047), PageSize::Set(1024));
}
