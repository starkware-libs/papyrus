mod block;
mod sources;
mod state;

use std::sync::Arc;
use std::time::Duration;

use futures_util::{pin_mut, select, StreamExt};
use indexmap::IndexMap;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tracing::{error, info, warn};

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};
use crate::block::{handle_block_reverts, store_block, stream_new_blocks};
use crate::state::{store_state_diff, stream_new_state_diffs};

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

#[allow(clippy::large_enum_variant)]
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
                Err(StateSyncError::ParentBlockHashMismatch {
                    block_number,
                    expected_parent_block_hash,
                    stored_parent_block_hash,
                }) => {
                    // A revert detected, log and restart sync loop.
                    info!(
                        "Detected revert while processing block {}. Parent hash of the incoming \
                         block is {}, current block hash is {}.",
                        block_number, expected_parent_block_hash, stored_parent_block_hash
                    );
                    continue;
                }
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
                Ok(_) => {
                    unreachable!("Sync should either return with an error or continue forever.")
                }
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
                StateSyncError::StateDiffWithoutMatchingHeader {
                    block_number: _,
                    block_hash: _,
                } => true,
                _ => false,
            }
        }
    }

    // Sync until encountering an error:
    //  1. If needed, revert blocks from the end of the chain.
    //  2. Create infinite block and state diff streams to fetch data from the central source.
    //  3. Fetch data from the streams with unblocking wait while there is no new data.
    async fn sync_while_ok(&mut self) -> StateSyncResult {
        let txn = self.writer.begin_rw_txn()?;
        handle_block_reverts(self.reader.clone(), txn, self.central_source.clone()).await?;
        let block_stream = stream_new_blocks(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.block_propagation_sleep_duration,
        )
        .fuse();
        let state_diff_stream = stream_new_state_diffs(
            self.reader.clone(),
            self.central_source.clone(),
            self.config.block_propagation_sleep_duration,
        )
        .fuse();
        pin_mut!(block_stream, state_diff_stream);

        loop {
            let sync_event = select! {
              res = block_stream.next() => res,
              res = state_diff_stream.next() => res,
              complete => break,
            }
            .expect("Received None as a sync event.")?;
            self.process_sync_event(sync_event).await?;
        }
        unreachable!("Fetching data loop should never return.");
    }

    // Tries to store the incoming data.
    async fn process_sync_event(&mut self, sync_event: SyncEvent) -> StateSyncResult {
        let txn = self.writer.begin_rw_txn()?;
        match sync_event {
            SyncEvent::BlockAvailable { block_number, block } => {
                store_block(self.reader.clone(), txn, block_number, block)
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            } => store_state_diff(
                self.reader.clone(),
                txn,
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            ),
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
