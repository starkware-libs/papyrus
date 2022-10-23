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
use starknet_api::block::{Block, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use starknet_client::ClientError;

pub use self::sources::{CentralError, CentralSource, CentralSourceConfig, CentralSourceTrait};

#[derive(Serialize, Deserialize)]
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
    CentralSourceError(#[from] ClientError),
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
        state_diff: StateDiff,
        // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
        // Class definitions of deployed contracts with classes that were not declared in this
        // state diff.
        deployed_contract_class_definitions: Vec<(ClassHash, ContractClass)>,
    },
}

async fn handle_reverts<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: &StorageReader,
    central_source: Arc<TCentralSource>,
) {
    let header_marker = reader
        .begin_ro_txn()
        .expect("Cannot read from block storage.")
        .get_header_marker()
        .expect("Cannot read from block storage.");

    // Revert last blocks if needed.
    let mut last_block_in_storage = header_marker.prev();
    while let Some(block_number) = last_block_in_storage {
        if should_revert_block(reader, central_source.clone(), block_number).await {
            revert_block(block_number);
            last_block_in_storage = block_number.prev();
        } else {
            break;
        }
    }
}

fn revert_block(block_number: BlockNumber) {
    unimplemented!("Revert block {}", block_number);
}

#[allow(clippy::new_without_default)]
impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub async fn run(&mut self) -> anyhow::Result<(), StateSyncError> {
        info!("State sync started.");
        loop {
            handle_reverts(&self.reader, self.central_source.clone()).await;
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
                let sync_event: Option<SyncEvent> = select! {
                  res = block_stream.next() => res,
                  res = state_diff_stream.next() => res,
                  complete => break,
                };
                match sync_event {
                    Some(SyncEvent::BlockAvailable { block_number, block }) => {
                        self.writer
                            .begin_rw_txn()?
                            .append_header(block_number, &block.header)?
                            .append_body(block_number, block.body)?
                            .commit()?;
                    }
                    Some(SyncEvent::StateDiffAvailable {
                        block_number,
                        state_diff,
                        deployed_contract_class_definitions,
                    }) => {
                        self.writer
                            .begin_rw_txn()?
                            .append_state_diff(
                                block_number,
                                state_diff,
                                deployed_contract_class_definitions,
                            )?
                            .commit()?;
                    }
                    None => {
                        return Err(StateSyncError::SyncError {
                            message: "Got an empty event.".to_string(),
                        });
                    }
                }
            }
        }
    }
}

/// Checks if centrals block hash at the block number is different from ours (or doesn't exist).
/// If so, a revert is required.
async fn should_revert_block<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: &StorageReader,
    central_source: Arc<TCentralSource>,
    block_number: BlockNumber,
) -> bool {
    if let Some(central_block_hash) =
        central_source.get_block_hash(block_number).await.expect("Cannot read from central.")
    {
        let storage_block_header = reader
            .begin_ro_txn()
            .expect("Cannot read from block storage.")
            .get_block_header(block_number)
            .expect("Cannot read from block storage.");

        match storage_block_header {
            Some(block_header) => block_header.block_hash != central_block_hash,
            None => false,
        }
    } else {
        // Block number doesn't exist in central, revert.
        true
    }
}

fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = SyncEvent> {
    stream! {
        loop {
            let header_marker = reader.begin_ro_txn().expect("Cannot read from block storage.")
                .get_header_marker()
                .expect("Cannot read from block storage.");

            let last_block_number = central_source
                .get_block_marker()
                .await
                .expect("Cannot read from central.");

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
                yield SyncEvent::BlockAvailable { block_number, block };
            }
        }
    }
}

fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = SyncEvent> {
    stream! {
        loop {
            let txn = reader.begin_ro_txn().expect("Cannot read from block storage.");
            let state_marker = txn
                .get_state_marker()
                .expect("Cannot read from block storage.");
            let last_block_number = txn
                .get_header_marker()
                .expect("Cannot read from block storage.");
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
                    Ok((block_number, mut state_diff, deployed_contract_class_definitions)) => {
                        sort_state_diff(&mut state_diff);
                        yield SyncEvent::StateDiffAvailable {
                            block_number,
                            state_diff,
                            deployed_contract_class_definitions,
                        }
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
