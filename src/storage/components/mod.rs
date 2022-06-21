mod block;
#[cfg(test)]
pub mod storage_test_utils;

use std::path::Path;

use serde::{Deserialize, Serialize};

pub use self::block::{
    open_block_storage, BlockStorageError, BlockStorageReader, BlockStorageWriter,
    HeaderStorageReader, HeaderStorageWriter,
};

#[derive(Serialize, Deserialize)]
pub struct StorageConfig {
    pub path: String,
}

pub struct StorageComponents {
    pub block_storage_reader: BlockStorageReader,
    pub block_storage_writer: BlockStorageWriter,
}

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error(transparent)]
    BlockStorageError(#[from] BlockStorageError),
}

impl StorageComponents {
    pub fn new(config: StorageConfig) -> Result<Self, StorageError> {
        let path = Path::new(&config.path);
        let (block_storage_reader, block_storage_writer) = open_block_storage(path)?;
        Ok(Self {
            block_storage_reader,
            block_storage_writer,
        })
    }
}
