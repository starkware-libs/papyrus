use tempfile::tempdir;

use super::{open_env, DbConfig, DbReader, DbWriter};

pub fn get_test_config() -> DbConfig {
    let dir = tempdir().unwrap();
    DbConfig {
        path: dir.path().to_str().unwrap().to_string(),
        max_size: 1 << 35, // 32GB.
    }
}
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
    let table_id = writer.create_table("table").unwrap();

    // Snapshot state by creating a read txn.
    let txn0 = reader.begin_ro_txn().unwrap();
    let table = txn0.open_table(&table_id).unwrap();

    // Insert a value.
    let wtxn = writer.begin_rw_txn().unwrap();
    wtxn.insert(&table, b"key", &b"data0".to_vec()).unwrap();
    wtxn.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn1 = reader.begin_ro_txn().unwrap();

    // Update the value.
    let wtxn = writer.begin_rw_txn().unwrap();
    wtxn.upsert(&table, b"key", &b"data1".to_vec()).unwrap();
    wtxn.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn2 = reader.begin_ro_txn().unwrap();

    // Validate data in snapshots.
    assert_eq!(txn0.get::<Vec<u8>>(&table, b"key").unwrap(), None);
    assert_eq!(
        txn1.get::<Vec<u8>>(&table, b"key").unwrap(),
        Some(b"data0".to_vec())
    );
    assert_eq!(
        txn2.get::<Vec<u8>>(&table, b"key").unwrap(),
        Some(b"data1".to_vec())
    );
}
