use tempfile::tempdir;

use super::{open_block_storage, BlockStorageReader, BlockStorageWriter};

pub fn get_test_storage() -> (BlockStorageReader, BlockStorageWriter) {
    let dir = tempdir().unwrap();
    open_block_storage(dir.path()).expect("Failed to open storage.")
}
