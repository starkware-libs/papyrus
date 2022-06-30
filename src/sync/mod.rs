mod sources;

use std::cmp::min;

use futures_util::StreamExt;
use futures_util::{pin_mut, select};
use log::{error, info};

use crate::starknet::{BlockHeader, BlockNumber, StateDiffForward};
use crate::starknet_client::ClientError;
use crate::storage::components::{
    BlockStorageError, BlockStorageReader, BlockStorageWriter, HeaderStorageReader,
    HeaderStorageWriter, StateStorageReader, StateStorageWriter,
};

pub use self::sources::{CentralSource, CentralSourceConfig};

// TODO(dan): Take from config.
const HEADER_CHUNKS: u64 = 100;

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and writes to Storage.
pub struct StateSync {
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
}
pub enum SyncEvent {
    HeaderAvailable {
        block_number: BlockNumber,
        header: BlockHeader,
    },
    StateDiffAvailable {
        block_number: BlockNumber,
        state_diff: StateDiffForward,
    },
    HeaderStreamExhausted,
    StateDiffStreamExhausted,
}

#[allow(clippy::new_without_default)]
impl StateSync {
    pub fn new(
        central_source: CentralSource,
        reader: BlockStorageReader,
        writer: BlockStorageWriter,
    ) -> StateSync {
        StateSync {
            central_source,
            reader,
            writer,
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<(), StateSyncError> {
        info!("State sync started.");
        loop {
            let header_marker = self.reader.get_header_marker()?;
            let state_marker = self.reader.get_state_marker()?;
            let last_block_number = self.central_source.get_block_number().await?;
            let upto_block_number = min(header_marker + HEADER_CHUNKS, last_block_number);
            let header_stream = self
                .central_source
                .stream_new_blocks(header_marker, upto_block_number)
                .fuse();
            info!(
                "Downloading headers [{} - {}].",
                header_marker.0, upto_block_number.0
            );
            let state_diff_stream = self
                .central_source
                .stream_state_updates(state_marker, header_marker)
                .fuse();
            info!(
                "Downloading state diffs [{} - {}).",
                state_marker.0, header_marker.0
            );
            pin_mut!(header_stream, state_diff_stream);

            let mut header_stream_exhausted = false;
            let mut state_diff_stream_exhausted = false;

            loop {
                let event = select! {
                    res = header_stream.next() => {
                        if let Some(event) = res {
                            SyncEvent::HeaderAvailable { block_number: event.0, header: event.1 }
                        } else {SyncEvent::HeaderStreamExhausted}
                    },
                    res = state_diff_stream.next() => {
                        if let Some(state_diff)= res{
                        SyncEvent::StateDiffAvailable { block_number: state_diff.0, state_diff: state_diff.1 }
                        } else {SyncEvent::StateDiffStreamExhausted}
                    },
                    complete => panic!("Should not be here.")
                };
                match event {
                    SyncEvent::HeaderAvailable {
                        block_number,
                        header,
                    } => {
                        // info!("SyncEvent::HeaderAvailable: {}.", block_number.0);
                        self.writer.append_header(block_number, &header)?;
                    }
                    SyncEvent::StateDiffAvailable {
                        block_number,
                        state_diff,
                    } => {
                        // info!("SyncEvent::StateDiffAvailable: {}.", block_number.0);
                        self.writer.append_state_diff(block_number, &state_diff)?;
                    }
                    SyncEvent::HeaderStreamExhausted => {
                        info!("Need more headers!");
                        header_stream_exhausted = true;
                    }
                    SyncEvent::StateDiffStreamExhausted => {
                        info!("Need more state diffs!");
                        state_diff_stream_exhausted = true;
                    }
                }
                if header_stream_exhausted && state_diff_stream_exhausted {
                    break;
                }
            }
        }
    }
}
