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

pub type StateSyncResult = Result<(), StateSyncError>;
#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    CentralSourceError(#[from] CentralError),
    #[error("Sync error: {message:?}.")]
    SyncError { message: String },
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
    pub async fn run(&mut self) -> StateSyncResult {
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
                let sync_event = match select! {
                  res = block_stream.next() => res,
                  res = state_diff_stream.next() => res,
                  complete => break,
                } {
                    Some(Ok(sync_event)) => sync_event,
                    Some(Err(err)) if is_recoverable(&err) => {
                        error!("{}", err);
                        // TODO: change sleep duration.
                        tokio::time::sleep(self.config.block_propagation_sleep_duration).await;
                        break;
                    }
                    Some(Err(err)) => {
                        error!("{}", err);
                        return Err(err);
                    }
                    None => {
                        unreachable!("Received None as a sync event.");
                    }
                };

                match self.process_sync_event(sync_event).await {
                    Ok(_) => {}
                    // A recoverable error occured, break the loop and create new streams.
                    Err(err) if is_recoverable(&err) => {
                        error!("{}", err);
                        break;
                    }
                    // Unrecoverable errors.
                    Err(err) => return Err(err),
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
                _ => false,
            }
        }
    }

    // Tries to store the incoming data.
    async fn process_sync_event(&mut self, sync_event: SyncEvent) -> StateSyncResult {
        match sync_event {
            SyncEvent::BlockAvailable { block_number, block } => {
                self.store_block(block_number, block)
            }
            SyncEvent::StateDiffAvailable {
                block_number,
                block_hash: _block_hash,
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

    fn store_block(&mut self, block_number: BlockNumber, block: Block) -> StateSyncResult {
        self.writer
            .begin_rw_txn()?
            .append_header(block_number, &block.header)?
            .append_body(block_number, block.body)?
            .commit()?;
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
            let header_marker = reader.begin_ro_txn()?
            .get_header_marker()?;

            let last_block_number = central_source
                .get_block_marker()
                .await?;

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
            while let Some(Ok((block_number, block))) = block_stream.next().await {
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
                match maybe_state_diff {
                    Ok((block_number, block_hash, mut state_diff, deployed_contract_class_definitions)) => {
                        sort_state_diff(&mut state_diff);
                        yield Ok(SyncEvent::StateDiffAvailable {
                            block_number,
                            block_hash,
                            state_diff,
                            deployed_contract_class_definitions,
                        })
                    }
                    Err(err) => {
                        error!("{}", err);
                        break;
                    }
                }
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
