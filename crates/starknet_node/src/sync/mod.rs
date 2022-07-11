mod sources;

use std::time::Duration;

use async_stream::stream;
use futures_util::{pin_mut, select, Stream, StreamExt};
use log::{error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockBody, BlockHeader, BlockNumber, StateDiffForward};
use starknet_client::ClientError;

pub use self::sources::{CentralSource, CentralSourceConfig};
use crate::storage::components::{
    BlockStorageError, BlockStorageReader, BlockStorageWriter, BodyStorageWriter,
    HeaderStorageReader, HeaderStorageWriter, StateStorageReader, StateStorageWriter,
};

#[derive(Serialize, Deserialize)]
pub struct SyncConfig {
    pub block_propagation_sleep_duration: Duration,
}

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct StateSync {
    config: SyncConfig,
    central_source: CentralSource,
    reader: BlockStorageReader,
    writer: BlockStorageWriter,
}

#[derive(thiserror::Error, Debug)]
pub enum StateSyncError {
    #[error(transparent)]
    StorageError(#[from] BlockStorageError),
    #[error(transparent)]
    CentralSourceError(#[from] ClientError),
    #[error("Sync error: {message:?}.")]
    SyncError { message: String },
}
pub enum SyncEvent {
    BlockAvailable { block_number: BlockNumber, header: BlockHeader, body: BlockBody },
    StateDiffAvailable { block_number: BlockNumber, state_diff: StateDiffForward },
}

#[allow(clippy::new_without_default)]
impl StateSync {
    pub fn new(
        config: SyncConfig,
        central_source: CentralSource,
        reader: BlockStorageReader,
        writer: BlockStorageWriter,
    ) -> StateSync {
        StateSync { config, central_source, reader, writer }
    }

    pub async fn run(&mut self) -> anyhow::Result<(), StateSyncError> {
        info!("State sync started.");
        loop {
            let header_stream = stream_new_blocks(
                self.reader.clone(),
                &self.central_source,
                self.config.block_propagation_sleep_duration,
            )
            .fuse();
            let state_diff_stream = stream_new_state_diffs(
                self.reader.clone(),
                &self.central_source,
                self.config.block_propagation_sleep_duration,
            )
            .fuse();
            pin_mut!(header_stream, state_diff_stream);

            loop {
                let sync_event: Option<SyncEvent> = select! {
                  res = header_stream.next() => res,
                  res = state_diff_stream.next() => res,
                  complete => break,
                };
                match sync_event {
                    Some(SyncEvent::BlockAvailable { block_number, header, body }) => {
                        self.writer
                            .begin_rw_txn()?
                            .append_header(block_number, &header)?
                            .append_body(block_number, &body)?
                            .commit()?;
                    }
                    Some(SyncEvent::StateDiffAvailable { block_number, state_diff }) => {
                        self.writer
                            .begin_rw_txn()?
                            .append_state_diff(block_number, &state_diff)?
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

fn stream_new_blocks(
    reader: BlockStorageReader,
    central_source: &CentralSource,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = SyncEvent> + '_ {
    stream! {
        loop {
            let header_marker = reader.begin_ro_txn().expect("Cannot read from block storage.")
                .get_header_marker()
                .expect("Cannot read from block storage.");
            let last_block_number = central_source
                .get_block_marker()
                .await
                .expect("Cannot read from block storage.");
            info!(
                "Downloading headers [{} - {}).",
                header_marker.0, last_block_number.0
            );
            if header_marker == last_block_number {
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
            let header_stream = central_source
                .stream_new_blocks(header_marker, last_block_number)
                .fuse();
            pin_mut!(header_stream);
            while let Some(Ok((block_number, header, body))) = header_stream.next().await {
                yield SyncEvent::BlockAvailable {
                    block_number,
                    header,
                    body,
                };
            }
        }
    }
}

fn stream_new_state_diffs(
    reader: BlockStorageReader,
    central_source: &CentralSource,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = SyncEvent> + '_ {
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
                state_marker.0, last_block_number.0
            );
            if state_marker == last_block_number {
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
            let header_stream = central_source
                .stream_state_updates(state_marker, last_block_number)
                .fuse();
            pin_mut!(header_stream);
            while let Some(Ok((block_number, state_diff))) = header_stream.next().await {
                yield SyncEvent::StateDiffAvailable {
                    block_number,
                    state_diff,
                };
            }
        }
    }
}
