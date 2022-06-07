use tempfile::tempdir;

use super::StorageComponents;

#[allow(dead_code)]
pub fn get_test_storage() -> StorageComponents {
    let dir = tempdir().unwrap();
    StorageComponents::new(dir.path()).expect("Failed to open test storage.")
}
