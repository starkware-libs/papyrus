mod block;
mod data;
mod downloads_manager;
mod sources;
mod state;
mod sync;

use std::sync::Arc;
use std::time::Duration;

use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};
use crate::block::BlockSync;
use crate::data::{BlockSyncData, StateDiffSyncData};
use crate::state::StateDiffSync;
use crate::sync::SingleDataTypeSync;

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
    #[error("Channel error - {msg}")]
    Channel { msg: String },
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) -> StateSyncResult {
        info!("State sync started.");
        let mut block_sync =
            SingleDataTypeSync::new(self.config, self.central_source.clone(), self.reader.clone())?;
        let mut state_diff_sync =
            SingleDataTypeSync::new(self.config, self.central_source.clone(), self.reader.clone())?;

        loop {
            match self.sync_while_ok(&mut block_sync, &mut state_diff_sync).await {
                // A recoverable error occurred. Sleep and try syncing again.
                Err(err) if is_recoverable(&err) => {
                    warn!("{}", err);
                    if let Err(err) = block_sync.restart() {
                        error!("{}", err);
                        return Err(err);
                    }
                    if let Err(err) = state_diff_sync.restart() {
                        error!("{}", err);
                        return Err(err);
                    }
                    tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                    continue;
                }
                // Unrecoverable errors.
                Err(err) => {
                    error!("{}", err);
                    return Err(err);
                }
                Ok(()) => continue,
            }
        }

        // Whitelisting of errors from which we might be able to recover.
        fn is_recoverable(err: &StateSyncError) -> bool {
            match err {
                StateSyncError::CentralSourceError(_) => true,
                StateSyncError::Channel { msg: _ } => true,
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
        block_sync: &mut SingleDataTypeSync<TCentralSource, BlockSyncData, BlockSync>,
        state_diff_sync: &mut SingleDataTypeSync<TCentralSource, StateDiffSyncData, StateDiffSync>,
    ) -> StateSyncResult {
        let txn1 = self.writer.begin_rw_txn()?;
        block_sync.step(txn1)?;

        let txn2 = self.writer.begin_rw_txn()?;
        state_diff_sync.step(txn2)?;

        tokio::time::sleep(Duration::from_millis(1)).await;
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
