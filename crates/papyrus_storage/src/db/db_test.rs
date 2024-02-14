use assert_matches::assert_matches;
use libmdbx::PageSize;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use crate::db::table_types::Table;
use crate::db::{get_page_size, open_env, DbError, DbIter, DbReader, DbResult, DbWriter};
use crate::serialization::serialization_traits::{
    NoVersionValueWrapper,
    ValueSerde,
    VersionZeroWrapper,
};
use crate::test_utils::get_test_config;

pub(crate) fn get_test_env() -> ((DbReader, DbWriter), TempDir) {
    let (config, temp_dir) = get_test_config(None);
    (open_env(&config.db_config).expect("Failed to open environment."), temp_dir)
}

#[test]
fn open_env_scenario() {
    get_test_env();
}

#[test]
fn open_env_with_enforce_file_exists() {
    let (config, _temp_dir) = get_test_config(None);
    let mut db_config = config.db_config;
    db_config.enforce_file_exists = true;

    // First call to `open_env` with `enforce_file_exists` set to `true` should fail because
    // the file does not exist yet. This equals to starting a new chain, where this flag must be
    // off.
    let result = open_env(&db_config);
    assert_matches!(result, Err(DbError::FileDoesNotExist(_)));

    // Make sure that file in the expected file indeed does not exist.
    let mut mdbx_file_exists = db_config.path().join("mdbx.dat").exists();
    assert_eq!(mdbx_file_exists, false);

    // Set `enforce_file_exists` to `false` and try again.
    // This equals to opening a new chain, where this flag is off.
    db_config.enforce_file_exists = false;

    // Second call to `open_env` should succeed and create the mdbx.dat file in the new env.
    // Called inside a block to drop the db handlers before the next call.
    {
        let result: DbResult<(DbReader, DbWriter)> = open_env(&db_config);
        assert_matches!(result, Ok(_));
    }

    // Ensure that mdbx.dat file exists in the expected location.
    // Third call with `enforce_file_exists` flag set to `true` should succeed.
    mdbx_file_exists = db_config.path().join("mdbx.dat").exists();
    assert_eq!(mdbx_file_exists, true);

    db_config.enforce_file_exists = true;
    let result: DbResult<(DbReader, DbWriter)> = open_env(&db_config);
    assert_matches!(result, Ok(_));

    // Add some charachter to the path to make it invalid.
    // Fourth and final call to `open_env` with path enforcement should fail because the path is
    // invalid.
    db_config.path_prefix = db_config.path_prefix.join("2");
    let result = open_env(&db_config);
    assert_matches!(result, Err(DbError::FileDoesNotExist(_)));
}

#[test]
fn txns_scenarios() {
    // Create an environment and a table.
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id =
        writer.create_simple_table::<[u8; 3], NoVersionValueWrapper<[u8; 5]>>("table").unwrap();

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
    let table_id =
        writer.create_simple_table::<[u8; 3], NoVersionValueWrapper<[u8; 5]>>("table").unwrap();

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

#[test]
fn test_iter() {
    // Create an environment and a table.
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id =
        writer.create_simple_table::<[u8; 4], NoVersionValueWrapper<[u8; 4]>>("table").unwrap();

    // Insert some values.
    let items = vec![
        (*b"key1", *b"val1"),
        (*b"key2", *b"val2"),
        (*b"key3", *b"val3"),
        (*b"key5", *b"val5"),
    ];
    let wtxn = writer.begin_rw_txn().unwrap();
    let table = wtxn.open_table(&table_id).unwrap();
    for (k, v) in &items {
        table.insert(&wtxn, k, v).unwrap();
    }
    wtxn.commit().unwrap();

    // Use the iterator.
    let txn = reader.begin_ro_txn().unwrap();
    let mut cursor = txn.open_table(&table_id).unwrap().cursor(&txn).unwrap();
    let iter = DbIter::new(&mut cursor);
    assert_eq!(items, iter.collect::<DbResult<Vec<_>>>().unwrap());

    let mut cursor = txn.open_table(&table_id).unwrap().cursor(&txn).unwrap();
    let mut iter = DbIter::new(&mut cursor);
    let mut index = 0;
    while let Some(Ok((k, v))) = iter.next() {
        assert_eq!(items[index], (k, v));
        index += 1;
    }
}

#[test]
fn with_version_zero_serialization() {
    let ((reader, mut writer), _temp_dir) = get_test_env();
    let table_id =
        writer.create_simple_table::<[u8; 4], VersionZeroWrapper<[u8; 4]>>("table").unwrap();

    let items = vec![
        (*b"key1", *b"val1"),
        (*b"key2", *b"val2"),
        (*b"key3", *b"val3"),
        (*b"key5", *b"val5"),
    ];
    let wtxn = writer.begin_rw_txn().unwrap();
    let table = wtxn.open_table(&table_id).unwrap();
    for (k, v) in &items {
        table.insert(&wtxn, k, v).unwrap();
    }
    wtxn.commit().unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let mut cursor = txn.open_table(&table_id).unwrap().cursor(&txn).unwrap();
    let iter = DbIter::new(&mut cursor);
    assert_eq!(items, iter.collect::<DbResult<Vec<_>>>().unwrap());

    // TODO: move to serialization tests.
    const A_RANDOM_U8: u8 = 123;
    let with_zero_version_serialization =
        VersionZeroWrapper::<u8>::serialize(&A_RANDOM_U8).unwrap();
    assert_eq!(with_zero_version_serialization, vec![0, 123]);
    assert_eq!(
        VersionZeroWrapper::<u8>::deserialize(&mut with_zero_version_serialization.as_slice()),
        Some(A_RANDOM_U8)
    );

    let with_no_version_serialization =
        NoVersionValueWrapper::<u8>::serialize(&A_RANDOM_U8).unwrap();
    assert_eq!(with_no_version_serialization, vec![123]);
    assert_eq!(
        NoVersionValueWrapper::<u8>::deserialize(&mut with_no_version_serialization.as_slice()),
        Some(A_RANDOM_U8)
    );
}
