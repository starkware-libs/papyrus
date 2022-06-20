mod sources;

use std::time::Duration;

use futures_util::pin_mut;
use log::{debug, error, info};
use tokio_stream::StreamExt;

use crate::starknet_client::ClientError;
use crate::storage::components::{
    BlockStorageError, BlockStorageReader, BlockStorageWriter, HeaderStorageReader,
    HeaderStorageWriter,
};

pub use self::sources::CentralSource;

// TODO(dan): Take from config.
const SLEEP_DURATION: Duration = Duration::from_millis(10000);

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
        loop {
            let initial_block_number = self.reader.get_header_marker()?;
            let last_block_number = self.central_source.get_block_number().await?;
            info!(
                "Syncing headers {} - {}.",
                initial_block_number.0, last_block_number.0
            );
            let stream = self
                .central_source
                .stream_new_blocks(initial_block_number, last_block_number);
            pin_mut!(stream);
            while let Some((number, header)) = stream.next().await {
                debug!("Received new header: {}.", number.0);
                self.writer.append_header(number, &header)?;
            }
            tokio::time::sleep(SLEEP_DURATION).await
        }
    }
}
