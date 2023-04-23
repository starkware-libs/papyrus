use assert_matches::assert_matches;

use crate::test_utils::get_test_storage;
use crate::version::{StorageVersionError, Version, VersionStorageReader, VersionStorageWriter};
use crate::{get_current_crate_version, StorageError};

#[tokio::test]
async fn version() {
    let (reader, mut writer) = get_test_storage();

    // No version initially - use crate version.
    let version = reader.begin_ro_txn().unwrap().get_version().unwrap();
    let crate_version = get_current_crate_version().expect("Storage crate should have a version");
    assert!(version.is_some());
    assert!(version.unwrap().0.eq(&crate_version.0));

    // Write and read version.
    let higher_version =
        Version(semver::Version { patch: crate_version.0.patch + 1, ..crate_version.0 });
    writer.begin_rw_txn().unwrap().set_version(higher_version.clone()).unwrap().commit().unwrap();
    let version = reader.begin_ro_txn().unwrap().get_version().unwrap();
    assert!(version.unwrap().0.eq(&higher_version.0));

    // Fail to set a version which is not higher than the existing one.
    if let Err(err) = writer.begin_rw_txn().unwrap().set_version(higher_version.clone()) {
        assert_matches!(
            err,
            StorageError::StorageVersion(StorageVersionError::SetLowerVersion {
                crate_version,
                storage_version
            })
            if crate_version.0.eq(&higher_version.0) && storage_version.0.eq(&higher_version.0)
        );
    } else {
        panic!("Unexpected Ok.");
    };
}
