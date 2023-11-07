use assert_matches::assert_matches;
use pretty_assertions::assert_eq;

use crate::test_utils::get_test_storage;
use crate::version::{StorageVersionError, Version, VersionStorageReader, VersionStorageWriter};
use crate::{StorageError, STORAGE_VERSION_STATE, STORAGE_VERSION_TRANSACTIONS};

#[tokio::test]
async fn version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // No version initially - use crate version.
    let version_state = reader.begin_ro_txn().unwrap().get_version_state().unwrap();
    let version_transactions = reader.begin_ro_txn().unwrap().get_version_transactions().unwrap();
    assert!(version_state.is_some());
    assert!(version_transactions.is_some());
    assert_eq!(version_state.unwrap(), STORAGE_VERSION_STATE);
    assert_eq!(version_transactions.unwrap(), STORAGE_VERSION_TRANSACTIONS);

    // Write and read version.
    let higher_version = Version(STORAGE_VERSION_STATE.0 + 1);
    writer.begin_rw_txn().unwrap().set_version_state(&higher_version).unwrap().commit().unwrap();
    let version_state = reader.begin_ro_txn().unwrap().get_version_state().unwrap();
    assert_eq!(version_state.unwrap(), higher_version);

    // Fail to set a version which is not higher than the existing one.
    let Err(err) = writer.begin_rw_txn().unwrap().set_version_state(&higher_version) else {
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
