use super::{open_storage, StorageReader, StorageWriter};
use crate::storage::db::db_test::get_test_config;

pub fn get_test_storage() -> (StorageReader, StorageWriter) {
    let config = get_test_config();
    open_storage(config).expect("Failed to open storage.")
}
