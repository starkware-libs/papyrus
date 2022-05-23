#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use crate::starknet::BlockNumber;

pub struct StorageError {}

pub trait StarknetStorageReader: Sync + Send {
    fn get_latest_block_number(&self) -> BlockNumber;
}

pub trait StarknetStorageWriter: Sync + Send {
    fn set_latest_block_number(&mut self, n: BlockNumber);
}

pub trait DataStore {
    type R: StarknetStorageReader;
    type W: StarknetStorageWriter;

    fn get_state_read_access(&self) -> Result<Self::R, StorageError>;

    fn get_state_write_access(&self) -> Result<Self::W, StorageError>;
}

// Storage object holding all the data. Owned by a single main thread. Responsible for doing all the
// non thread safe operations.
struct ConcreteDataStore {
    latest_block_num: BlockNumber,
}

pub struct DataStoreHandle {
    inner: Arc<Mutex<ConcreteDataStore>>,
}

pub struct SNStorageReader {
    store: Arc<Mutex<ConcreteDataStore>>,
}

impl StarknetStorageReader for SNStorageReader {
    fn get_latest_block_number(&self) -> BlockNumber {
        return self.store.lock().unwrap().latest_block_num;
    }
}

pub struct SNStorageWriter {
    store: Arc<Mutex<ConcreteDataStore>>,
}

impl StarknetStorageWriter for SNStorageWriter {
    fn set_latest_block_number(&mut self, n: BlockNumber) {
        self.store.lock().unwrap().latest_block_num = n;
    }
}

impl DataStore for DataStoreHandle {
    type R = SNStorageReader;
    type W = SNStorageWriter;

    fn get_state_read_access(&self) -> Result<SNStorageReader, StorageError> {
        Ok(SNStorageReader {
            store: self.inner.clone(),
        })
    }

    fn get_state_write_access(&self) -> Result<SNStorageWriter, StorageError> {
        Ok(SNStorageWriter {
            store: self.inner.clone(),
        })
    }
}
