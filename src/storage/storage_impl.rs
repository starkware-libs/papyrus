use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::api::{StarknetStorageReader, StarknetStorageWriter, StorageError};

use crate::starknet::BlockNumber;

/**
 * The concrete data store implementation.
 * This should be the single implementation, shared by different threads.
 */
pub struct TheDataStore {
    pub latest_block_num: BlockNumber,
}

impl TheDataStore {
    pub fn new() -> TheDataStore {
        TheDataStore {
            latest_block_num: BlockNumber(0),
        }
    }
}

pub struct SNStorageReader {
    pub store: Arc<Mutex<TheDataStore>>,
}

#[async_trait]
impl StarknetStorageReader for SNStorageReader {
    async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        Ok(self.store.lock().await.latest_block_num)
    }
}

pub struct SNStorageWriter {
    pub store: Arc<Mutex<TheDataStore>>,
}

#[async_trait]
impl StarknetStorageWriter for SNStorageWriter {
    async fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError> {
        self.store.lock().await.latest_block_num = n;
        Ok(())
    }
}
