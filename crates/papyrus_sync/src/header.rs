use std::sync::Arc;
use std::time::Duration;

use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::{Block, BlockNumber};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::{debug, info};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult};

pub async fn sync_block_while_ok<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    writer: Arc<Mutex<StorageWriter>>,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
) -> StateSyncResult {
    loop {
        let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
        let last_block_number = central_source.get_block_marker().await?;
        if header_marker == last_block_number {
            tokio::time::sleep(block_propagation_sleep_duration).await;
            continue;
        }

        info!("Downloading blocks [{} - {}).", header_marker, last_block_number);
        let block_stream = central_source.stream_new_blocks(header_marker, last_block_number);
        tokio::pin!(block_stream);

        while let Some(maybe_block) = block_stream.next().await {
            match maybe_block {
                Ok((block_number, block)) => {
                    if !store_block_or_revert(writer.clone(), block_number, block).await? {
                        break;
                    }
                }
                Err(err) => {
                    return Err(StateSyncError::CentralSourceError(err));
                }
            }
        }
    }
}

pub async fn store_block_or_revert(
    writer: Arc<Mutex<StorageWriter>>,
    block_number: BlockNumber,
    block: Block,
) -> Result<bool, StateSyncError> {
    let mut locked_writer = writer.lock().await;
    let txn = locked_writer.begin_rw_txn()?;

    // Compares the block's parent hash to the stored block and reverts the previous block if
    // there's a discrepancy.
    if let Some(prev_block_number) = block_number.prev() {
        let prev_header =
            txn.get_block_header(prev_block_number)?.ok_or(StorageError::DBInconsistency {
                msg: format!(
                    "Missing block {} in the storage (for verifying block {}).",
                    prev_block_number, block_number
                ),
            })?;

        if prev_header.block_hash != block.header.parent_hash {
            info!("Reverting block {}.", prev_header.block_number);
            txn.revert_header(prev_header.block_number)?
                .insert_ommer_header(prev_header.block_hash, &prev_header)?
                .revert_body(prev_header.block_number)?
                .revert_state_diff(prev_header.block_number)?
                .0
                .commit()?;

            return Ok(false);
        }
    };

    debug!("Storing block: {}.", block.header.block_number);
    txn.append_header(block_number, &block.header)?
        .append_body(block_number, block.body)?
        .commit()?;

    Ok(true)
}
