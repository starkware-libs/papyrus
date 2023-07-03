use assert_matches::assert_matches;

use crate::test_utils::get_test_storage;
use crate::version::{StorageVersionError, Version, VersionStorageReader, VersionStorageWriter};
use crate::{StorageError, STORAGE_VERSION};

#[tokio::test]
async fn version() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // No version initially - use crate version.
    let version = reader.begin_ro_txn().unwrap().get_version().unwrap();
    assert!(version.is_some());
    assert_eq!(version.unwrap(), STORAGE_VERSION);

    // Write and read version.
    let higher_version = Version(STORAGE_VERSION.0 + 1);
    writer.begin_rw_txn().unwrap().set_version(&higher_version).unwrap().commit().unwrap();
    let version = reader.begin_ro_txn().unwrap().get_version().unwrap();
    assert_eq!(version.unwrap(), higher_version);

    // Fail to set a version which is not higher than the existing one.
    if let Err(err) = writer.begin_rw_txn().unwrap().set_version(&higher_version) {
        assert_matches!(
            err,
            StorageError::StorageVersionInconcistency(StorageVersionError::SetLowerVersion {
                crate_version,
                storage_version
            })
            if crate_version == higher_version && storage_version == higher_version
        );
    } else {
        panic!("Unexpected Ok.");
    };
}
