use tempfile::tempdir;

use crate::db::DbConfig;
use crate::{open_storage, StorageReader, StorageWriter};

pub fn get_test_config() -> DbConfig {
    let dir = tempdir().unwrap();
    DbConfig {
        path: dir.path().to_str().unwrap().to_string(),
        max_size: 1 << 35, // 32GB.
    }
}

pub fn get_test_storage() -> (StorageReader, StorageWriter) {
    let config = get_test_config();
    open_storage(config).unwrap()
}
