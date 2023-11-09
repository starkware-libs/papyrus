use assert_matches::assert_matches;
use pretty_assertions::assert_eq;

use crate::test_utils::get_test_storage;
use crate::version::{StorageVersionError, Version, VersionStorageReader, VersionStorageWriter};
use crate::{
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

#[tokio::test]
async fn test_verify_storage_version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let higher_version = Version(STORAGE_VERSION_STATE.0 + 1);

    verify_storage_version(reader.clone(), StorageScope::FullArchive).unwrap();
    verify_storage_version(reader.clone(), StorageScope::StateOnly).unwrap();

    writer.begin_rw_txn().unwrap().set_blocks_version(&higher_version).unwrap().commit().unwrap();
    verify_storage_version(reader.clone(), StorageScope::FullArchive)
        .expect_err("Should fail, because storage blocks version does not match.");
    verify_storage_version(reader.clone(), StorageScope::StateOnly).unwrap();

    writer.begin_rw_txn().unwrap().set_state_version(&higher_version).unwrap().commit().unwrap();
    verify_storage_version(reader.clone(), StorageScope::FullArchive)
        .expect_err("Should fail, because both versions do not match.");
    verify_storage_version(reader, StorageScope::StateOnly)
        .expect_err("Should fail, because state blocks version does not match.");
}
