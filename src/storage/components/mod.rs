mod block;

use std::path::Path;

pub use self::block::open_block_storage;
use self::block::{BlockStorageReader, BlockStorageWriter};

pub struct StorageComponents {
    pub block_storage_reader: BlockStorageReader,
    pub block_storage_writer: BlockStorageWriter,
}

#[allow(dead_code)]
impl StorageComponents {
    pub fn new(path: &Path) -> Self {
        let (block_storage_reader, block_storage_writer) = open_block_storage(path).unwrap();
        Self {
            block_storage_reader,
            block_storage_writer,
        }
    }
}
