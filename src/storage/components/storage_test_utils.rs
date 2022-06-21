use crate::storage::db::db_test::get_test_config;

use super::{StorageComponents, StorageConfig};

#[allow(dead_code)]
pub fn get_test_storage() -> StorageComponents {
    let config = StorageConfig {
        db_config: get_test_config(),
    };
    StorageComponents::new(config).expect("Failed to open test storage.")
}
