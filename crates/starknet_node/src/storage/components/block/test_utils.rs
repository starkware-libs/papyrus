use super::{open_block_storage, BlockStorageReader, BlockStorageWriter};
use crate::storage::db::db_test::get_test_config;

pub fn get_test_storage() -> (BlockStorageReader, BlockStorageWriter) {
    let config = get_test_config();
    open_block_storage(config).expect("Failed to open storage.")
}
