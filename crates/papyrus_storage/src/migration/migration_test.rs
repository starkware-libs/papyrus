use std::path::PathBuf;

use starknet_api::block::{BlockHash, BlockNumber};
use tempfile::{tempdir, TempDir};
use tracing_test::traced_test;

use crate::db::{open_env, DbConfig, TableIdentifier};
use crate::test_utils::get_test_config;
use crate::version::VersionStorageReader;
use crate::{open_storage, STORAGE_VERSION};

fn create_test_storage_v0() -> TempDir {
    let dir = tempdir().unwrap();
    let db_config = DbConfig {
        path: dir.path().to_path_buf(),
        min_size: 1 << 20,    // 1MB
        max_size: 1 << 35,    // 32GB
        growth_step: 1 << 26, // 64MB
    };

    let (_, mut db_writer) = open_env(db_config).unwrap();
    let test_table_v0_id: TableIdentifier<BlockHash, BlockNumber> =
        db_writer.create_table("test_table_v0").unwrap();

    let txn = db_writer.begin_rw_txn().unwrap();
    let test_table_v0 = txn.open_table(&test_table_v0_id).unwrap();
    test_table_v0.insert(&txn, &BlockHash::default(), &BlockNumber::default()).unwrap();
    txn.commit().unwrap();

    dir
}

unsafe fn migrate_v0_to_v1(path: PathBuf) {
    let db_config = DbConfig {
        path,
        min_size: 1 << 20,    // 1MB
        max_size: 1 << 35,    // 32GB
        growth_step: 1 << 26, // 64MB
    };

    let (_, mut db_writer) = open_env(db_config).unwrap();

    // Simulate migration of v0 -> v1 where each value gets the next BlockNumber.
    let test_table_v0_id: TableIdentifier<BlockHash, BlockNumber> =
        db_writer.create_table("test_table_v0").unwrap();
    let test_table_v1_id: TableIdentifier<BlockHash, BlockNumber> =
        db_writer.create_table("test_table_v1").unwrap();

    let txn = db_writer.begin_rw_txn().unwrap();
    let test_table_v0 = txn.open_table(&test_table_v0_id).unwrap();
    let test_table_v1 = txn.open_table(&test_table_v1_id).unwrap();

    let mut v0_cursor = test_table_v0.cursor(&txn).unwrap();
    while let Some((key, value_v0)) = v0_cursor.next().unwrap() {
        let value_v1 = value_v0.next();
        test_table_v1.insert(&txn, &key, &value_v1).unwrap();
    }
    txn.commit().unwrap();

    // drop table v0
    db_writer.drop_table("test_table_v0").unwrap();
}

#[test]
fn storage_v0_creation() {
    let dir = create_test_storage_v0();
    let db_config = DbConfig {
        path: dir.path().to_path_buf(),
        min_size: 1 << 20,    // 1MB
        max_size: 1 << 35,    // 32GB
        growth_step: 1 << 26, // 64MB
    };

    let (db_reader, mut db_writer) = open_env(db_config).unwrap();
    let test_table_v0_id: TableIdentifier<BlockHash, BlockNumber> =
        db_writer.create_table("test_table_v0").unwrap();
    let txn = db_reader.begin_ro_txn().unwrap();
    let block_number =
        txn.open_table(&test_table_v0_id).unwrap().get(&txn, &BlockHash::default()).unwrap();

    assert_eq!(block_number, Some(BlockNumber::default()));
}

#[test]
fn migration_simulation() {
    let dir = create_test_storage_v0();
    unsafe {
        migrate_v0_to_v1(dir.path().to_path_buf());
    }

    let db_config = DbConfig {
        path: dir.path().to_path_buf(),
        min_size: 1 << 20,    // 1MB
        max_size: 1 << 35,    // 32GB
        growth_step: 1 << 26, // 64MB
    };
    let (db_reader, mut db_writer) = open_env(db_config).unwrap();
    // Test that v0 table was dropped.
    db_reader.get_table_stats("test_table_v0").unwrap_err();

    let v1_stats = db_reader.get_table_stats("test_table_v1").unwrap();
    assert_eq!(v1_stats.entries, 1);

    let test_table_v1_id: TableIdentifier<BlockHash, BlockNumber> =
        db_writer.create_table("test_table_v1").unwrap();
    let txn = db_reader.begin_ro_txn().unwrap();
    let block_number =
        txn.open_table(&test_table_v1_id).unwrap().get(&txn, &BlockHash::default()).unwrap();

    assert_eq!(block_number, Some(BlockNumber::default().next()));
}

#[traced_test]
#[test]
fn v0_upgrade() {
    let db_config = get_test_config();
    let (_, mut writer) = open_storage(db_config.clone()).unwrap();
    unsafe {
        writer.db_writer.drop_table("storage_version").unwrap();
    }
    drop(writer);

    // Calls migrate_db that will call to_v0 since we dropped the storage_version table.
    let (reader, _) = open_storage(db_config).unwrap();
    let db_version = reader.begin_ro_txn().unwrap().get_version().unwrap();
    assert_eq!(db_version, Some(STORAGE_VERSION));
}
