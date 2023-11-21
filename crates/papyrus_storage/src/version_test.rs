use assert_matches::assert_matches;
use pretty_assertions::assert_eq;

use crate::test_utils::{get_test_storage, get_test_storage_by_scope};
use crate::version::{StorageVersionError, Version, VersionStorageReader, VersionStorageWriter};
use crate::{
    set_version_if_needed,
    verify_storage_version,
    StorageError,
    StorageScope,
    STORAGE_VERSION_BLOCKS,
    STORAGE_VERSION_STATE,
};

#[tokio::test]
async fn version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // No version initially - use crate version.
    let version_state = reader.begin_ro_txn().unwrap().get_state_version().unwrap();
    let version_blocks = reader.begin_ro_txn().unwrap().get_blocks_version().unwrap();
    assert!(version_state.is_some());
    assert!(version_blocks.is_some());
    assert_eq!(version_state.unwrap(), STORAGE_VERSION_STATE);
    assert_eq!(version_blocks.unwrap(), STORAGE_VERSION_BLOCKS);

    // Write and read version.
    let higher_version = Version(STORAGE_VERSION_STATE.0 + 1);
    writer.begin_rw_txn().unwrap().set_state_version(&higher_version).unwrap().commit().unwrap();
    let version_state = reader.begin_ro_txn().unwrap().get_state_version().unwrap();
    assert_eq!(version_state.unwrap(), higher_version);

    // Fail to set a version which is not higher than the existing one.
    let Err(err) = writer.begin_rw_txn().unwrap().set_state_version(&higher_version) else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::StorageVersionInconcistency(StorageVersionError::SetLowerVersion {
            crate_version,
            storage_version
        })
        if crate_version == higher_version && storage_version == higher_version
    );
}

#[test]
fn test_verify_storage_version_good_flow() {
    let ((reader_full_archive, _), _temp_dir) =
        get_test_storage_by_scope(StorageScope::FullArchive);
    let ((reader_state_only, _), _temp_dir) = get_test_storage_by_scope(StorageScope::StateOnly);
    verify_storage_version(reader_full_archive.clone()).unwrap();
    verify_storage_version(reader_state_only.clone()).unwrap();
}

#[test]
fn test_verify_storage_version_different_blocks_version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_by_scope(StorageScope::FullArchive);
    let blocks_higher_version = Version(STORAGE_VERSION_BLOCKS.0 + 1);
    writer
        .begin_rw_txn()
        .unwrap()
        .set_blocks_version(&blocks_higher_version)
        .unwrap()
        .commit()
        .unwrap();
    assert_matches!(
        verify_storage_version(reader.clone()),
        Err(StorageError::StorageVersionInconcistency(
            StorageVersionError::InconsistentStorageVersion {
                crate_version: STORAGE_VERSION_BLOCKS,
                storage_version: _,
            },
        ))
    );
}

#[test]
fn test_verify_storage_version_different_state_version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_by_scope(StorageScope::FullArchive);
    let state_higher_version = Version(STORAGE_VERSION_STATE.0 + 1);
    writer
        .begin_rw_txn()
        .unwrap()
        .set_state_version(&state_higher_version)
        .unwrap()
        .commit()
        .unwrap();
    assert_matches!(
        verify_storage_version(reader),
        Err(StorageError::StorageVersionInconcistency(
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
        set_version_if_needed(reader.clone(), writer).is_err(),
        "Should fail, because storage scope cannot shift from state-only to full-archive."
    );
}
