pub mod header;
pub mod state;

mod sources;

use std::sync::Arc;
use std::time::Duration;

use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
// use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::header::sync_block_while_ok;
pub use crate::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};
use crate::state::sync_state_while_ok;

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
    writer: Arc<Mutex<StorageWriter>>,
}

pub type StateSyncResult = Result<(), StateSyncError>;

#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
    #[error(transparent)]
    TokioJoinError(#[from] tokio::task::JoinError),
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) {
        info!("State sync started.");

        loop {
            let block_sync_task = sync_block_while_ok(
                self.reader.clone(),
                self.writer.clone(),
                self.central_source.clone(),
                self.config.block_propagation_sleep_duration,
            );
            // let block_sync = tokio::spawn(block_sync_task);
            let state_sync_task = sync_state_while_ok(
                self.reader.clone(),
                self.writer.clone(),
                self.central_source.clone(),
                self.config.block_propagation_sleep_duration,
            );
            // let state_sync = tokio::spawn(state_sync_task);
            // match tokio::try_join!(flatten(block_sync), flatten(state_sync)) {
            //     Err(err) => error!("{}", err),
            //     Ok(_) => unreachable!("Should sync while ok."),
            // }
            tokio::select! {
                res = block_sync_task => error!("{}", res.unwrap_err()),
                res = state_sync_task => error!("{}", res.unwrap_err()),
            }
            tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
        }
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
        Self {
            config,
            central_source: Arc::new(central_source),
            reader,
            writer: Arc::new(Mutex::new(writer)),
        }
    }
}

// async fn flatten(handle: JoinHandle<StateSyncResult>) -> StateSyncResult {
//     match handle.await {
//         Ok(Ok(())) => Ok(()),
//         Ok(Err(err)) => Err(err),
//         Err(err) => Err(StateSyncError::TokioJoinError(err)),
//     }
// }
