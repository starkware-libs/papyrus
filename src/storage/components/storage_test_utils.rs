use tempfile::tempdir;

use super::{StorageComponents, StorageConfig};

#[allow(dead_code)]
pub fn get_test_storage() -> StorageComponents {
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        path: dir.path().to_str().unwrap().to_string(),
    };
    StorageComponents::new(config).expect("Failed to open test storage.")
}
