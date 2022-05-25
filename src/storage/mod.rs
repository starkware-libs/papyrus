#[cfg(test)]
mod tests;

use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::starknet::BlockNumber;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Synchronization error")]
    AccessSyncError {},
}

/**
 * An interface to an object that reads from the starknet storage.
 */
#[async_trait]
pub trait StarknetStorageReader: Sync + Send {
    async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError>;
}

/**
 * An interface to an object writing to a the starknet storage.
 */
#[async_trait]
pub trait StarknetStorageWriter: Sync + Send {
    async fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError>;
}

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
 * An interface to an object the provides access (read and write) to the Starknet storage.
 * Specific implementations should specialized this with specific reader/writer implementations.
 *
 * See #StarknetStorageReader, #StarknetStorageWriter
 *
 */
trait DataStore<R, W>
where
    R: StarknetStorageReader,
    W: StarknetStorageWriter,
{
    fn get_access(&self) -> Result<(R, W), StorageError>;
}

/**
 * The concrete data store implementation.
 * This should be the single implementation, shared by different threads.
 */
struct TheDataStore {
    latest_block_num: BlockNumber,
}

/**
 * A handle to a #TheDataStore
 */
pub struct DataStoreHandle {
    inner: Arc<Mutex<TheDataStore>>,
}

impl DataStoreHandle {
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

impl DataStore<SNStorageReader, SNStorageWriter> for DataStoreHandle {
    fn get_access(&self) -> Result<(SNStorageReader, SNStorageWriter), StorageError> {
        Ok((
            self.get_state_read_access()?,
            self.get_state_write_access()?,
        ))
    }
}

/**
 * This is the function that's supposed to be called by the function that initializes
 * the store and wires it to relevant other modules.
 */
pub fn create_store_access() -> Result<DataStoreHandle, StorageError> {
    //TODO: find a way to limit calls to this function
    let ds = TheDataStore {
        latest_block_num: BlockNumber(0),
    };
    let dsh = DataStoreHandle {
        inner: Arc::new(Mutex::new(ds)),
    };
    Ok(dsh)
}
