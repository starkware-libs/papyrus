mod block;
mod sources;
mod state;

use std::sync::Arc;
use std::time::Duration;

use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{error, info, warn};

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};
use crate::block::BlockSync;
use crate::state::StateDiffSync;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct SyncConfig {
    pub block_propagation_sleep_duration: Duration,
    pub recoverable_error_sleep_duration: Duration,
}

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct GenericStateSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    writer: StorageWriter,
}

pub type StateSyncResult = Result<(), StateSyncError>;

#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
    #[error("Sync internal error - {msg}")]
    SyncInternalError { msg: String },
    #[error(
        "Parent block hash of block {block_number} is not consistent with the stored block. \
         Expected {expected_parent_block_hash}, found {stored_parent_block_hash}."
    )]
    ParentBlockHashMismatch {
        block_number: BlockNumber,
        expected_parent_block_hash: BlockHash,
        stored_parent_block_hash: BlockHash,
    },
    #[error(
        "Received state diff of block {block_number} and block hash {block_hash}, didn't find a \
         matching header (neither in the ommer headers)."
    )]
    StateDiffWithoutMatchingHeader { block_number: BlockNumber, block_hash: BlockHash },
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) -> StateSyncResult {
        info!("State sync started.");
        let mut block_sync =
            BlockSync::new(self.config, self.central_source.clone(), self.reader.clone());
        let mut state_diff_sync =
            StateDiffSync::new(self.config, self.central_source.clone(), self.reader.clone());

        loop {
            match self.sync_while_ok(&mut block_sync, &mut state_diff_sync).await {
                // A recoverable error occurred. Sleep and try syncing again.
                Err(err) if is_recoverable(&err) => {
                    warn!("{}", err);
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    continue;
                }
                // Unrecoverable errors.
                Err(err) => {
                    error!("{}", err);
                    return Err(err);
                }
                Ok(_) => continue,
            }
        }

        // Whitelisting of errors from which we might be able to recover.
        fn is_recoverable(err: &StateSyncError) -> bool {
            match err {
                StateSyncError::CentralSourceError(_) => true,
                StateSyncError::StorageError(storage_err)
                    if matches!(storage_err, StorageError::InnerError(_)) =>
                {
                    true
                }
                _ => false,
            }
        }
    }

    async fn sync_while_ok(
        &mut self,
        block_sync: &mut BlockSync<TCentralSource>,
        state_diff_sync: &mut StateDiffSync<TCentralSource>,
    ) -> StateSyncResult {
        let txn1 = self.writer.begin_rw_txn()?;
        block_sync.step(txn1)?;

        let txn2 = self.writer.begin_rw_txn()?;
        state_diff_sync.step(txn2)?;

        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        Ok(())
    }
}

pub type StateSync = GenericStateSync<CentralSource>;

impl StateSync {
    pub fn new(
        config: SyncConfig,
        central_source: CentralSource,
        reader: StorageReader,
        writer: StorageWriter,
    ) -> Self {
        Self { config, central_source: Arc::new(central_source), reader, writer }
    }
}
