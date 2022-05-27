use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::api::{StorageError, StorageReader, StorageWriter};

use crate::starknet::BlockNumber;

/**
 * The concrete data store implementation.
 * This should be the single implementation, shared by different threads.
 */
pub struct TheDataStore {
    pub latest_block_num: BlockNumber,
}

pub struct SNStorageReader {
    pub store: Arc<Mutex<TheDataStore>>,
}

pub struct SNStorageWriter {
    pub store: Arc<Mutex<TheDataStore>>,
}

impl TheDataStore {
    pub fn new() -> TheDataStore {
        TheDataStore {
            latest_block_num: BlockNumber(0),
        }
    }
}

impl Default for TheDataStore {
    fn default() -> Self {
        TheDataStore::new()
    }
}

#[async_trait]
impl StorageReader for SNStorageReader {
    async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        Ok(self.store.lock().await.latest_block_num)
    }
}

#[async_trait]
impl StorageWriter for SNStorageWriter {
    async fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError> {
        self.store.lock().await.latest_block_num = n;
        Ok(())
    }
}
