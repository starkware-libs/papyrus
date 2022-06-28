mod sources;

use std::cmp::min;
use std::time::Duration;

use futures_util::pin_mut;
use log::{debug, error, info};
use reqwest::StatusCode;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::starknet_client::ClientError;
use crate::storage::components::{
    BlockStorageError, BlockStorageReader, BlockStorageWriter, HeaderStorageReader,
    HeaderStorageWriter, StateStorageReader, StateStorageWriter,
};

pub use self::sources::{CentralSource, CentralSourceConfig};

// TODO(dan): Take from config.
const SLEEP_DURATION: Duration = Duration::from_millis(10000);
const SYNC_CHUNKS: usize = 40;

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
        let (tx, mut rx) = mpsc::channel(SYNC_CHUNKS);
        let harmonize_reader = self.reader.clone();
        let central = self.central_source.clone();

        tokio::spawn(async move {
            loop {
                let mut state_marker = harmonize_reader
                    .get_state_marker()
                    .expect("Cannot read state marker.");
                let header_marker = harmonize_reader
                    .get_header_marker()
                    .expect("Cannot read header marker.");
                while state_marker < header_marker {
                    let res = central.get_state_update(state_marker).await;
                    match res {
                        Ok(su) => {
                            info!("Spawned received new block: {}.", state_marker.0);
                            if (tx.send((state_marker, su)).await).is_err() {
                                println!("receiver dropped");
                                return;
                            }
                        }
                        Err(err) => {
                            debug!("Received error for block {}: {:?}.", state_marker.0, err);
                            // TODO(dan): proper error handling.
                            match err {
                                ClientError::BadResponse { status } => {
                                    if status == StatusCode::TOO_MANY_REQUESTS {
                                        // TODO(dan): replace with a retry mechanism.
                                        debug!("Waiting for 5 sec.");
                                        tokio::time::sleep(Duration::from_millis(5000)).await;
                                    } else {
                                        error!("{:?}", err);
                                        todo!()
                                    }
                                }
                                ClientError::RequestError(err) => {
                                    error!("{:?}", err);
                                    todo!()
                                }
                                ClientError::SerdeError(err) => {
                                    error!("{:?}", err);
                                    todo!()
                                }
                                ClientError::StarknetError(err) => {
                                    error!("{:?}", err);
                                    todo!()
                                }
                            }
                        }
                    }

                    state_marker = state_marker.next()
                }
            }
        });

        loop {
            while let Ok((block_number, state_diff)) = rx.try_recv() {
                info!("Received new state diff: {}.", block_number.0);
                self.writer.append_state_diff(block_number, &state_diff)?;
            }

            let initial_block_number = self.reader.get_header_marker()?;
            let last_block_number = self.central_source.get_block_number().await?;
            let up_to_block_number = min(
                last_block_number,
                initial_block_number + SYNC_CHUNKS.try_into().unwrap(),
            );
            info!(
                "Syncing headers {} - {}.",
                initial_block_number.0, up_to_block_number.0
            );
            let stream = self
                .central_source
                .stream_new_blocks(initial_block_number, up_to_block_number);
            pin_mut!(stream);
            while let Some((number, header)) = stream.next().await {
                debug!("Received new header: {}.", number.0);
                self.writer.append_header(number, &header)?;
            }
            tokio::time::sleep(SLEEP_DURATION).await
        }
    }
}
