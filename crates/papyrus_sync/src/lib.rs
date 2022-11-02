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
use starknet_api::{Block, BlockNumber, DeclaredContract, StateDiff};
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
        deployed_contract_class_definitions: Vec<DeclaredContract>,
    },
    BlocksStreamConsumed,
    StateUpdatesStreamConsumed,
}

enum SyncStatus {
    FullySynced(BlockNumber),
    Syncing { blocks_marker: BlockNumber, state_marker: BlockNumber, central_marker: BlockNumber },
}

#[allow(clippy::new_without_default)]
impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> GenericStateSync<TCentralSource> {
    pub(crate) async fn iteration(
        &mut self,
        blocks_marker: BlockNumber,
        central_marker: BlockNumber,
    ) -> anyhow::Result<(), StateSyncError> {
        info!("Creating streams for fetching blocks and state updates.");
        let block_stream =
            stream_new_blocks(self.central_source.clone(), blocks_marker, central_marker).fuse();
        let state_diff_stream = stream_new_state_diffs(
            self.reader.clone(),
            self.central_source.clone(),
            central_marker,
            self.config.block_propagation_sleep_duration,
        )
        .fuse();
        pin_mut!(block_stream, state_diff_stream);

        info!("Getting next sync event from streams.");
        loop {
            let sync_event: Option<SyncEvent> = select! {
                res = block_stream.next() => {
                    match res {
                        None => Some(SyncEvent::BlocksStreamConsumed),
                        _ => res,
                    }
                },
                res = state_diff_stream.next() => {
                    match res {
                        None => Some(SyncEvent::StateUpdatesStreamConsumed),
                        _ => res,
                    }
                },
                complete => break,
            };
            match sync_event {
                Some(SyncEvent::BlockAvailable { block_number, block }) => {
                    info!("Block {} available.", block_number);
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
                    info!("State diff {} available.", block_number);
                    self.writer
                        .begin_rw_txn()?
                        .append_state_diff(
                            block_number,
                            state_diff,
                            deployed_contract_class_definitions,
                        )?
                        .commit()?;
                }
                Some(SyncEvent::BlocksStreamConsumed) => {
                    info!("Consumed the blocks stream.");
                }
                Some(SyncEvent::StateUpdatesStreamConsumed) => {
                    info!("Consumed the state updates stream.");
                }
                None => {
                    return Err(StateSyncError::SyncError {
                        message: "Got an empty event.".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    async fn sync_status(&self) -> anyhow::Result<SyncStatus, StateSyncError> {
        let central_marker = self.central_source.get_block_marker().await?;
        let blocks_marker = self.reader.begin_ro_txn()?.get_header_marker()?;
        let state_marker = self.reader.begin_ro_txn()?.get_state_marker()?;
        if blocks_marker == central_marker {
            return Ok(SyncStatus::FullySynced(central_marker));
        }
        Ok(SyncStatus::Syncing { blocks_marker, state_marker, central_marker })
    }

    pub async fn run(&mut self) -> anyhow::Result<(), StateSyncError> {
        info!("Sync starting.");
        loop {
            let sync_status = self.sync_status().await?;
            match sync_status {
                SyncStatus::FullySynced(block_number) => {
                    info!("Fully synced up to block {}", block_number);
                    break;
                }
                SyncStatus::Syncing { blocks_marker, state_marker, central_marker } => {
                    info!(
                        "Sync status: central currently at block {}, blocks at {}, state updates at {}.",
                        central_marker, blocks_marker, state_marker
                    );
                    self.iteration(blocks_marker, central_marker).await?;
                    tokio::time::sleep(self.config.block_propagation_sleep_duration).await;
                }
            }
        }
        Ok(())
    }
}

fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    central_source: Arc<TCentralSource>,
    header_marker: BlockNumber,
    last_block_number: BlockNumber,
) -> impl Stream<Item = SyncEvent> {
    stream! {
        info!("Creating stream for fetching blocks from central ({} up to {})", header_marker, last_block_number);
        let block_stream = central_source
            .stream_new_blocks(header_marker, last_block_number)
            .fuse();
        pin_mut!(block_stream);
        while let Some(Ok((block_number, block))) = block_stream.next().await {
            yield SyncEvent::BlockAvailable { block_number, block };
        }
    }
}

fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    last_block_number: BlockNumber,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = SyncEvent> {
    stream! {
        loop {
            let state_marker = reader.begin_ro_txn().expect("Cannot read from block storage.").get_state_marker()
                .expect("Cannot read from block storage.");

            info!(
                "Downloading state diffs [{} - {}).",
                state_marker, last_block_number
            );
            if state_marker == last_block_number {
                info!("State marker reached block {}, stop streaming", last_block_number);
                break;
            }
            let blocks_marker = reader.begin_ro_txn().expect("Cannot read from block storage.").get_header_marker().expect("Cannot read from block storage.");
            if state_marker == blocks_marker {
                info!("State marker caught with block marker, waiting for blocks marker to advance");
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue
            }
            let state_diff_stream = central_source
                .stream_state_updates(state_marker, blocks_marker)
                .fuse();
            pin_mut!(state_diff_stream);
            while let Some(maybe_state_diff) = state_diff_stream.next().await {
                match maybe_state_diff {
                    Ok((block_number, state_diff, deployed_contract_class_definitions)) => {
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
