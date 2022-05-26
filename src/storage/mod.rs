mod api;
#[cfg(test)]
mod storage_tests;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use self::api::{StarknetStorageReader, StarknetStorageWriter, StorageError};
use crate::starknet::BlockNumber;

pub struct SNStorageReader {
    store: Arc<Mutex<TheDataStore>>,
}

#[async_trait]
impl StarknetStorageReader for SNStorageReader {
    async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        Ok(self.store.lock().await.latest_block_num)
    }
}

pub struct SNStorageWriter {
    store: Arc<Mutex<TheDataStore>>,
}

#[async_trait]
impl StarknetStorageWriter for SNStorageWriter {
    async fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError> {
        self.store.lock().await.latest_block_num = n;
        Ok(())
    }
}

/**
 * The concrete data store implementation.
 * This should be the single implementation, shared by different threads.
 */
struct TheDataStore {
    latest_block_num: BlockNumber,
}

/**
 * This is the function that's supposed to be called by the function that initializes
 * the store and wires it to relevant other modules.
 */
pub fn create_store_access() -> Result<(SNStorageReader, SNStorageWriter), StorageError> {
    //TODO: find a way to limit calls to this function

    let ds = TheDataStore {
        latest_block_num: BlockNumber(0),
    };

    let m = Arc::new(Mutex::new(ds));

    let r = SNStorageReader { store: m.clone() };

    let w = SNStorageWriter { store: m.clone() };

    Ok((r, w))
}
