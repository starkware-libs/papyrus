use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::api::{StorageError, StorageReader, StorageWriter};

use crate::starknet::BlockNumber;

/**
 * This is the function that's supposed to be called by the function that initializes
 * the store and wires it to relevant other modules.
 */
pub fn create_store_access() -> Result<(SNStorageReader, SNStorageWriter), StorageError> {
    let ds = NodeDataStore::new();

    let m = Arc::new(Mutex::new(ds));

    let r = SNStorageReader { store: m.clone() };
    let w = SNStorageWriter { store: m };

    Ok((r, w))
}

/**
 * The concrete data store implementation.
 * This should be the single implementation, shared by different threads.
 */
pub struct NodeDataStore {
    pub latest_block_num: BlockNumber,
}

pub struct SNStorageReader {
    pub store: Arc<Mutex<NodeDataStore>>,
}

pub struct SNStorageWriter {
    pub store: Arc<Mutex<NodeDataStore>>,
}

impl NodeDataStore {
    pub fn new() -> NodeDataStore {
        NodeDataStore {
            latest_block_num: BlockNumber(0),
        }
    }
}

impl Default for NodeDataStore {
    fn default() -> Self {
        NodeDataStore::new()
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
