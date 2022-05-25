#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use thiserror::Error;

use crate::starknet::BlockNumber;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Synchronization error")]
    AccessSyncError {},
}

impl From<PoisonError<MutexGuard<'_, TheDataStore>>> for StorageError {
    fn from(_: PoisonError<MutexGuard<TheDataStore>>) -> Self {
        StorageError::AccessSyncError {}
    }
}
/**
 * An interface to an object that reads from the starknet storage.
 */
pub trait StarknetStorageReader: Sync + Send {
    fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError>;
}

/**
 * An interface to an object writing to a the starknet storage.
 */
pub trait StarknetStorageWriter: Sync + Send {
    fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError>;
}

pub struct SNStorageReader {
    store: Arc<Mutex<TheDataStore>>,
}

impl StarknetStorageReader for SNStorageReader {
    fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        Ok(self.store.lock()?.latest_block_num)
    }
}

pub struct SNStorageWriter {
    store: Arc<Mutex<TheDataStore>>,
}

impl StarknetStorageWriter for SNStorageWriter {
    fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError> {
        self.store.lock()?.latest_block_num = n;
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
trait DataStore {
    type R: StarknetStorageReader;
    type W: StarknetStorageWriter;

    fn get_access(&self) -> Result<(Self::R, Self::W), StorageError>;
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

impl DataStore for DataStoreHandle {
    type R = SNStorageReader;
    type W = SNStorageWriter;

    fn get_access(&self) -> Result<(SNStorageReader, SNStorageWriter), StorageError> {
        Ok((
            self.get_state_read_access()?,
            self.get_state_write_access()?,
        ))
    }
}
