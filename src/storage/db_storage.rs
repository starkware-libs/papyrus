use async_trait::async_trait;

use crate::starknet::{BlockBody, BlockHash, BlockHeader};

use super::api::{StorageError, StorageHandle};

// Storage object holding all the data. Owned by a single main thread. Responsible for doing all the
// non thread safe operations.
pub struct Storage {}

// Handle object that holds no storage data. Multiple instances might be held by different threads.
// Represents an interface for multiple consumers to read and write to the storage.
#[derive(Clone)]
pub struct StorageHandleImpl {}

pub fn create_storage() -> Result<(Storage, StorageHandleImpl), StorageError> {
    todo!("Not implemented yet.");
}

#[async_trait]
impl StorageHandle for StorageHandleImpl {
    async fn add_block_header(
        &self,
        _block_header: BlockHeader,
    ) -> Result<BlockHash, StorageError> {
        todo!("Not implemented yet.");
    }
    async fn add_block_body(
        &self,
        _block_id: BlockHash,
        _block_body: BlockBody,
    ) -> Result<(), StorageError> {
        todo!("Not implemented yet.");
    }
    async fn get_block_header(&self, _block_id: BlockHash) -> Result<BlockHeader, StorageError> {
        todo!("Not implemented yet.");
    }
}
