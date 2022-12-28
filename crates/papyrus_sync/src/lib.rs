#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;

mod sources;

use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures_util::{pin_mut, select, Stream, StreamExt};
use log::{error, info};
use papyrus_storage::{
    BodyStorageWriter, HeaderStorageReader, HeaderStorageWriter, StateStorageReader,
    StateStorageWriter, StorageError, StorageReader, StorageWriter,
};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct SyncConfig {
    pub block_propagation_sleep_duration: Duration,
}

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct GenericStateSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    writer: StorageWriter,
}

#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
    #[error("Sync error: {message:?}.")]
    SyncError { message: String },
    #[error(
        "Parent block hash of block {block_number:?} is not consistent with the stored block. \
         Expected {expected_parent_block_hash:?}, found {stored_parent_block_hash:?}."
    )]
    ParentBlockHashMismatch {
        block_number: BlockNumber,
        expected_parent_block_hash: BlockHash,
        stored_parent_block_hash: BlockHash,
    },
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
        deployed_contract_class_definitions: Vec<(ClassHash, ContractClass)>,
    },
}

#[allow(clippy::new_without_default)]
impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) -> anyhow::Result<(), StateSyncError> {
        info!("State sync started.");
        loop {
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
                .expect("Sync event should not be None.")?;

                match self.process_sync_event(sync_event).await {
                    Ok(_) => {}
                    Err(StateSyncError::ParentBlockHashMismatch {
                        block_number,
                        expected_parent_block_hash: _,
                        stored_parent_block_hash: _,
                    }) => {
                        info!("Detected revert while processing block {}", block_number);
                        break;
                    }
                    Err(err) if is_recoverable(&err) => {
                        error!("{}", err);
                        break;
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
        }

        fn is_recoverable(err: &StateSyncError) -> bool {
            match err {
                StateSyncError::CentralSourceError(_) => true,
                StateSyncError::SyncError { message: _ } => true,
                StateSyncError::StorageError(storage_err)
                    if matches!(storage_err, StorageError::InnerError(_)) =>
                {
                    true
                }
                _ => false,
            }
        }
    }

    // Tries to store the incoming data.
    async fn process_sync_event(&mut self, sync_event: SyncEvent) -> Result<(), StateSyncError> {
        match sync_event {
            SyncEvent::BlockAvailable { block_number, block } => {
                self.store_block(block_number, block).await
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash: _,
                state_diff,
                deployed_contract_class_definitions,
            } => {
                self.writer
                    .begin_rw_txn()?
                    .append_state_diff(
                        block_number,
                        state_diff,
                        deployed_contract_class_definitions,
                    )?
                    .commit()?;
                Ok(())
            }
        }
    }

    async fn store_block(
        &mut self,
        block_number: BlockNumber,
        block: Block,
    ) -> Result<(), StateSyncError> {
        // Assuming the central source is trusted, detect reverts by comparing the incoming block's
        // parent hash to the current hash.
        self.verify_parent_block_hash(block_number, &block).await?;

        self.writer
            .begin_rw_txn()?
            .append_header(block_number, &block.header)?
            .append_body(block_number, block.body)?
            .commit()?;
        Ok(())
    }

    // Compares the block's parent hash to the stored block.
    async fn verify_parent_block_hash(
        &self,
        block_number: BlockNumber,
        block: &Block,
    ) -> Result<(), StateSyncError> {
        let prev_block_number = match block_number.prev() {
            None => return Ok(()),
            Some(bn) => bn,
        };
        let prev_hash = self
            .reader
            .begin_ro_txn()?
            .get_block_header(prev_block_number)?
            .ok_or(StorageError::DBInconsistency {
                msg: format!(
                    "Missing block {} in the storage (for verifing block {}).",
                    prev_block_number, block_number
                ),
            })?
            .block_hash;

        if prev_hash != block.header.parent_hash {
            return Err(StateSyncError::ParentBlockHashMismatch {
                block_number,
                expected_parent_block_hash: block.header.parent_hash,
                stored_parent_block_hash: prev_hash,
            });
        }

        Ok(())
    }
}

fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    stream! {
        loop {
            let header_marker = reader.begin_ro_txn()?.get_header_marker()?;

            let last_block_number = central_source
                .get_block_marker()
                .await.map_err(|e| CentralError::ClientError(Arc::new(e)))?;

            info!(
                "Downloading blocks [{} - {}).",
                header_marker, last_block_number
            );
            if header_marker == last_block_number {
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
            let block_stream = central_source
                .stream_new_blocks(header_marker, last_block_number)
                .fuse();
            pin_mut!(block_stream);
            while let Some(maybe_block) = block_stream.next().await {
                let (block_number, block) = maybe_block?;
                yield Ok(SyncEvent::BlockAvailable { block_number, block });
            }
        }
    }
}

fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let state_marker = txn.get_state_marker()?;
            let last_block_number = txn.get_header_marker()?;
            drop(txn);
            info!(
                "Downloading state diffs [{} - {}).",
                state_marker, last_block_number
            );
            if state_marker == last_block_number {
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
            let state_diff_stream = central_source
                .stream_state_updates(state_marker, last_block_number)
                .fuse();
            pin_mut!(state_diff_stream);
            while let Some(maybe_state_diff) = state_diff_stream.next().await {
                let (block_number, block_hash, mut state_diff, deployed_contract_class_definitions) = maybe_state_diff?;
                sort_state_diff(&mut state_diff);
                yield Ok(SyncEvent::StateDiffAvailable {
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                });
            }
        }
    }
}

pub fn sort_state_diff(diff: &mut StateDiff) {
    diff.declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
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
