use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::db::RW;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageError, StorageReader, StorageTxn};
use starknet_api::block::{Block, BlockHeader};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, info, trace};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, SyncEvent};

pub fn sync_block_while_ok<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            debug!("Started sync_block_while_ok.");
            let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
            let last_block_number = central_source.get_block_marker().await?;
            if header_marker == last_block_number {
                debug!("Waiting for more blocks.");
                tokio::time::sleep(block_propagation_sleep_duration).await;
                continue;
            }

            info!("Downloading blocks [{header_marker} - {last_block_number}).");
            let block_stream = central_source.stream_new_blocks(header_marker, last_block_number).fuse();
            for await maybe_block in block_stream {
                let (_, block) = maybe_block?;
                yield SyncEvent::BlockAvailable { block };
            }
        }
    }
}

pub fn process_block_event(
    txn: StorageTxn<'_, RW>,
    reader: StorageReader,
    block: Block,
) -> Result<(StorageTxn<'_, RW>, bool), StateSyncError> {
    let (in_chain, maybe_parent) = parent_in_chain(reader, &block.header)?;
    if in_chain {
        return Ok((store_block(txn, block)?, false));
    } else {
        // Cannot fail - see parent_in_chain.
        let parent = maybe_parent.unwrap();
        return Ok((revert_block(txn, parent)?, true));
    }
}

fn revert_block(
    txn: StorageTxn<'_, RW>,
    header: BlockHeader,
) -> Result<StorageTxn<'_, RW>, StateSyncError> {
    info!("Reverting block {}.", header.block_number);
    trace!("Block header {header:#?}");
    Ok(txn
        .revert_header(header.block_number)?
        .insert_ommer_header(header.block_hash, &header)?
        .revert_body(header.block_number)?
        .revert_state_diff(header.block_number)?
        .0)
}

fn store_block(
    txn: StorageTxn<'_, RW>,
    block: Block,
) -> Result<StorageTxn<'_, RW>, StateSyncError> {
    info!(
        "Storing block header {} with hash {}.",
        block.header.block_number, block.header.block_hash
    );
    trace!("Block: {block:#?}");
    Ok(txn
        .append_header(block.header.block_number, &block.header)?
        .append_body(block.header.block_number, block.body)?)
}

fn parent_in_chain(
    reader: StorageReader,
    header: &BlockHeader,
) -> Result<(bool, Option<BlockHeader>), StateSyncError> {
    if let Some(prev_block_number) = header.block_number.prev() {
        let prev_header = reader.begin_ro_txn()?.get_block_header(prev_block_number)?.ok_or(
            StorageError::DBInconsistency {
                msg: format!(
                    "Missing block {prev_block_number} in the storage (for verifying block {}).",
                    header.block_number
                ),
            },
        )?;
        return Ok((prev_header.block_hash == header.parent_hash, Some(prev_header)));
    }

    Ok((true, None))
}
