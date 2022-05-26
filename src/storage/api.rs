use async_trait::async_trait;
use thiserror::Error;

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
pub trait StorageReader: Sync + Send {
    async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError>;
}

/**
 * An interface to an object writing to a the starknet storage.
 */
#[async_trait]
pub trait StorageWriter: Sync + Send {
    async fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError>;
}

/**
 * An interface to an object the provides access (read and write) to the Starknet storage.
 * Specific implementations should specialized this with specific reader/writer implementations.
 *
 * See #StorageReader, #StorageWriter
 *
 */
pub trait DataStore<R, W>
where
    R: StorageReader,
    W: StorageWriter,
{
    fn get_access(&self) -> Result<(R, W), StorageError>;
}
