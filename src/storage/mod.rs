use crate::starknet::{BlockBody, BlockHash, BlockHeader};

// Storage object holding all the data. Owned by a single main thread. Responsible for doing all the
// non thread safe operations.
pub struct Storage {}

// Handle object that holds no storage data. Multiple instances might be held by different threads.
// Represents an interface for multiple consumers to read and write to the storage.
#[derive(Clone)]
pub struct StorageHandle {}

pub struct StorageError {}

pub fn create_storage() -> Result<(Storage, StorageHandle), StorageError> {
    todo!("Not implemented yet.");
}

impl StorageHandle {
    pub fn add_block_header(&self, _block_header: BlockHeader) -> Result<BlockHash, StorageError> {
        todo!("Not implemented yet.");
    }
    pub fn add_block_body(
        &self,
        _block_id: BlockHash,
        _block_body: BlockBody,
    ) -> Result<(), StorageError> {
        todo!("Not implemented yet.");
    }
    pub fn get_block_header(&self, _block_id: BlockHash) -> BlockHeader {
        todo!("Not implemented yet.");
    }
}
