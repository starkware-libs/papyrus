mod sources;

use futures_util::pin_mut;
use log::info;
use tokio_stream::StreamExt;

use crate::starknet_client::ClientError;
use crate::storage::components::{
    BlockStorageError, BlockStorageReader, BlockStorageWriter, HeaderStorageReader,
    HeaderStorageWriter,
};

pub use self::sources::CentralSource;

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
        let initial_block_number = self.reader.get_header_marker()?;
        let stream = self.central_source.stream_new_blocks(initial_block_number);
        pin_mut!(stream);
        while let Some((number, header)) = stream.next().await {
            info!("Received new block number: {:?}.", number);
            self.writer.append_header(number, &header)?;
        }

        Ok(())
    }
}
