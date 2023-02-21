mod block;
mod sources;
mod state;

use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};
use crate::block::{run_block_sync, store_block};
use crate::state::{run_state_diff_sync, store_state_diff};

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
    SenderError(#[from] mpsc::error::SendError<SyncEvent>),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SyncEvent {
    BlockAvailable {
        block_number: BlockNumber,
        block: Block,
    },
    StateDiffAvailable {
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
        // Class definitions of deployed contracts with classes that were not declared in this
        // state diff.
        deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
    },
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) -> StateSyncResult {
        info!("State sync started.");
        loop {
            match self.sync_while_ok().await {
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
                Ok(()) => continue,
            }
        }

        // Whitelisting of errors from which we might be able to recover.
        fn is_recoverable(err: &StateSyncError) -> bool {
            if let StateSyncError::StorageError(storage_err) = err {
                if !matches!(storage_err, StorageError::InnerError(_)) {
                    return false;
                }
            }

            true
        }
    }

    // Sync until encountering an error:
    //  1. If needed, revert blocks from the end of the chain.
    //  2. Create infinite block and state diff streams to fetch data from the central source.
    //  3. Fetch data from the streams with unblocking wait while there is no new data.
    async fn sync_while_ok(&mut self) -> StateSyncResult {
        let (block_sender, mut sync_event_receiver) = mpsc::channel(100);
        let state_sender = block_sender.clone();

        let block_stream = run_block_sync(
            self.config,
            self.central_source.clone(),
            self.reader.clone(),
            block_sender,
        );
        tokio::spawn(block_stream);
        let state_diff_stream = run_state_diff_sync(
            self.config,
            self.central_source.clone(),
            self.reader.clone(),
            state_sender,
        );
        tokio::spawn(state_diff_stream);

        while let Some(sync_event) = sync_event_receiver.recv().await {
            self.process_sync_event(sync_event).await?;
            debug!("Finished processing sync event.");
        }

        Ok(())
    }

    // Tries to store the incoming data.
    async fn process_sync_event(&mut self, sync_event: SyncEvent) -> StateSyncResult {
        let txn = self.writer.begin_rw_txn()?;
        match sync_event {
            SyncEvent::BlockAvailable { block_number, block } => {
                debug!("Got block sync event.");
                store_block(
                    self.reader.clone(),
                    txn,
                    block_number,
                    block,
                    self.central_source.clone(),
                )
                .await
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            } => {
                debug!("Got state diff sync event.");
                store_state_diff(
                    self.reader.clone(),
                    txn,
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                )
            }
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
        Self { config, central_source: Arc::new(central_source), reader, writer }
    }
}
