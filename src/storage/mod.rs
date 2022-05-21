use crate::starknet::{BlockBody, BlockHash, BlockHeader, BlockNumber};

pub mod tests;

// Storage object holding all the data. Owned by a single main thread. Responsible for doing all the
// non thread safe operations.
#[derive(Clone, Copy)]
pub struct Storage {
    latest_block_num : BlockNumber
}

impl Storage
{
    fn set_latest_block_number(&mut self, n : BlockNumber) {
        self.latest_block_num = n;
    }
}

// Handle object that holds no storage data. Multiple instances might be held by different threads.
// Represents an interface for multiple consumers to read and write to the storage.
#[derive(Clone)]
pub struct StorageHandle {
    storage : Storage

}

pub struct StorageError {}

pub fn create_storage() -> Result<StorageHandle, StorageError> {

    let s = Storage { latest_block_num : BlockNumber(0)};
    let sh = StorageHandle { storage : s };
    return Ok(sh)
}

impl StorageHandle {


    pub fn new(s : &Storage) -> StorageHandle {
        return StorageHandle { storage : *s};
    }

    pub fn storage(&self) -> Storage { return self.storage; }

    pub fn set_latest_block_number(&self, n : BlockNumber) {
        self.storage().set_latest_block_number(n);

    }

    pub fn get_latest_block_number(&self) -> BlockNumber {
        return self.storage().latest_block_num;
    }

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


