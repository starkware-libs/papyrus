use assert_matches::assert_matches;
use pretty_assertions::assert_eq;

use crate::db::table_types::Table;
use crate::test_utils::{get_test_config, get_test_storage, get_test_storage_by_scope};
use crate::version::{
    StorageVersionError,
    Version,
    VersionStorageReader,
    VersionStorageWriter,
    VERSION_BLOCKS_KEY,
    VERSION_STATE_KEY,
};
use crate::{
    open_storage,
    set_version_if_needed,
    verify_storage_version,
    StorageError,
    StorageScope,
    STORAGE_VERSION_BLOCKS,
    STORAGE_VERSION_STATE,
};

// TODO: Add this test for set_blocks_version or combine the logic.
#[test]
fn set_state_version_test() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // No version initially - use crate version.
    let version_state = reader.begin_ro_txn().unwrap().get_state_version().unwrap();
    let version_blocks = reader.begin_ro_txn().unwrap().get_blocks_version().unwrap();
    assert!(version_state.is_some());
    assert!(version_blocks.is_some());
    assert_eq!(version_state.unwrap(), STORAGE_VERSION_STATE);
    assert_eq!(version_blocks.unwrap(), STORAGE_VERSION_BLOCKS);

    // Write and read version.
    let higher_minor_version =
        Version { major: STORAGE_VERSION_STATE.major, minor: STORAGE_VERSION_STATE.minor + 1 };
    writer
        .begin_rw_txn()
        .unwrap()
        .set_state_version(&higher_minor_version)
        .unwrap()
        .commit()
        .unwrap();
    let version_state = reader.begin_ro_txn().unwrap().get_state_version().unwrap();
    assert_eq!(version_state.unwrap(), higher_minor_version);

    // Fail to set a version which its minor not higher than the existing one.
    let crate_storage_version =
        Version { major: STORAGE_VERSION_STATE.major, minor: STORAGE_VERSION_STATE.minor };
    let Err(err) = writer.begin_rw_txn().unwrap().set_state_version(&crate_storage_version) else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::StorageVersionInconsistency(StorageVersionError::SetLowerVersion {
            crate_version,
            storage_version
        })
        if crate_version == crate_storage_version && storage_version == higher_minor_version
    );

    // Fail to set a version which its major is different.
    let different_major_version =
        Version { major: higher_minor_version.major + 1, minor: higher_minor_version.minor };
    let Err(err) = writer.begin_rw_txn().unwrap().set_state_version(&different_major_version)
    else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::StorageVersionInconsistency(StorageVersionError::SetMajorVersion {
            crate_version,
            storage_version
        })
        if crate_version == different_major_version && storage_version == higher_minor_version
    );
}

#[test]
fn version_migration() {
    let ((reader, mut writer), temp_dir) = get_test_storage();

    // Set the storage version on a lower minor version.
    let wtxn = writer.begin_rw_txn().unwrap();
    let version_table = wtxn.open_table(&wtxn.tables.storage_version).unwrap();
    version_table
        .upsert(
            &wtxn.txn,
            &VERSION_STATE_KEY.to_string(),
            &Version { major: STORAGE_VERSION_STATE.major, minor: 0 },
        )
        .unwrap();
    version_table
        .upsert(
            &wtxn.txn,
            &VERSION_BLOCKS_KEY.to_string(),
            &Version { major: STORAGE_VERSION_BLOCKS.major, minor: 0 },
        )
        .unwrap();
    wtxn.commit().unwrap();
    drop(reader);
    drop(writer);

    // Reopen the storage and verify the version.
    let (mut config, _) = get_test_config(None);
    config.db_config.path_prefix = temp_dir.path().to_path_buf();
    let (reader, _) = open_storage(config).unwrap();

    let version_state = reader.begin_ro_txn().unwrap().get_state_version().unwrap();
    assert_eq!(version_state.unwrap(), STORAGE_VERSION_STATE);
    let version_blocks = reader.begin_ro_txn().unwrap().get_blocks_version().unwrap();
    assert_eq!(version_blocks.unwrap(), STORAGE_VERSION_BLOCKS);
}

#[test]
fn open_storage_failed_different_major_versions() {
    let ((reader, mut writer), temp_dir) = get_test_storage();

    // Set the storage version on a different major version.
    // We can be sure that the major version in the code is less than u32::MAX.
    let high_major_version = Version { major: u32::MAX, minor: 0 };
    let wtxn = writer.begin_rw_txn().unwrap();
    let version_table = wtxn.open_table(&wtxn.tables.storage_version).unwrap();
    version_table.upsert(&wtxn.txn, &VERSION_STATE_KEY.to_string(), &high_major_version).unwrap();
    version_table.upsert(&wtxn.txn, &VERSION_BLOCKS_KEY.to_string(), &high_major_version).unwrap();
    wtxn.commit().unwrap();
    drop(reader);
    drop(writer);

    // Reopen the storage and verify the version.
    let (mut config, _) = get_test_config(None);
    config.db_config.path_prefix = temp_dir.path().to_path_buf();

    let Err(err) = open_storage(config) else {
        panic!("Unexpected Ok.");
    };
    assert_matches!(
        err,
        StorageError::StorageVersionInconsistency(StorageVersionError::InconsistentStorageVersion {
            crate_version,
            storage_version
        })
        if crate_version == STORAGE_VERSION_STATE && storage_version == high_major_version
    );
}

#[test]
fn test_verify_storage_version_good_flow() {
    let ((reader_full_archive, _), _temp_dir) =
        get_test_storage_by_scope(StorageScope::FullArchive);
    let ((reader_state_only, _), _temp_dir) = get_test_storage_by_scope(StorageScope::StateOnly);
    verify_storage_version(reader_full_archive).unwrap();
    verify_storage_version(reader_state_only).unwrap();
}

#[test]
fn test_verify_storage_version_different_minor_blocks_version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_by_scope(StorageScope::FullArchive);
    let blocks_higher_version =
        Version { major: STORAGE_VERSION_BLOCKS.major, minor: STORAGE_VERSION_BLOCKS.minor + 1 };
    writer
        .begin_rw_txn()
        .unwrap()
        .set_blocks_version(&blocks_higher_version)
        .unwrap()
        .commit()
        .unwrap();
    assert_matches!(
        verify_storage_version(reader),
        Err(StorageError::StorageVersionInconsistency(
            StorageVersionError::InconsistentStorageVersion {
                crate_version: STORAGE_VERSION_BLOCKS,
                storage_version: _,
            },
        ))
    );
}

#[test]
fn test_verify_storage_version_different_minor_state_version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_by_scope(StorageScope::FullArchive);
    let state_higher_version =
        Version { major: STORAGE_VERSION_STATE.major, minor: STORAGE_VERSION_STATE.minor + 1 };
    writer
        .begin_rw_txn()
        .unwrap()
        .set_state_version(&state_higher_version)
        .unwrap()
        .commit()
        .unwrap();
    assert_matches!(
        verify_storage_version(reader),
        Err(StorageError::StorageVersionInconsistency(
            StorageVersionError::InconsistentStorageVersion {
                crate_version: STORAGE_VERSION_STATE,
                storage_version: _,
            },
        ))
    );
}

#[test]
fn test_set_version_if_needed() {
    let ((mut reader, mut writer), _temp_dir) = get_test_storage_by_scope(StorageScope::StateOnly);
    reader.scope = StorageScope::FullArchive;
    writer.scope = StorageScope::FullArchive;
    assert!(
        set_version_if_needed(reader, writer).is_err(),
        "Should fail, because storage scope cannot shift from state-only to full-archive."
    );
}
