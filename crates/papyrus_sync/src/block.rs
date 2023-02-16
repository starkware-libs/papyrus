use std::sync::Arc;
use std::time::Duration;

use futures_util::{pin_mut, StreamExt};
use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
use papyrus_storage::db::RW;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageError, StorageReader, StorageTxn, TransactionIndex};
use starknet_api::block::{Block, BlockNumber};
use starknet_api::transaction::TransactionOffsetInBlock;
use tokio::sync::mpsc;
use tracing::{debug, info, trace};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncEvent};

pub(crate) fn store_block(
    reader: StorageReader,
    txn: StorageTxn<'_, RW>,
    block_number: BlockNumber,
    block: Block,
) -> StateSyncResult {
    // Assuming the central source is trusted, detect reverts by comparing the incoming block's
    // parent hash to the current hash.
    verify_parent_block_hash(reader, block_number, &block)?;

    debug!("Storing block {block_number} with hash {}.", block.header.block_hash);
    trace!("Block data: {block:#?}");
    txn.append_header(block_number, &block.header)?
        .append_body(block_number, block.body)?
        .commit()?;
    Ok(())
}

// Compares the block's parent hash to the stored block.
fn verify_parent_block_hash(
    reader: StorageReader,
    block_number: BlockNumber,
    block: &Block,
) -> StateSyncResult {
    let prev_block_number = match block_number.prev() {
        None => return Ok(()),
        Some(bn) => bn,
    };
    let prev_hash = reader
        .begin_ro_txn()?
        .get_block_header(prev_block_number)?
        .ok_or(StorageError::DBInconsistency {
            msg: format!(
                "Missing block {prev_block_number} in the storage (for verifying block \
                 {block_number}).",
            ),
        })?
        .block_hash;

    if prev_hash != block.header.parent_hash {
        return Err(StateSyncError::ParentBlockHashMismatch {
            block_number,
            expected_parent_block_hash: block.header.parent_hash,
            stored_parent_block_hash: prev_hash,
        });
    }

    Ok(())
}

// Reverts data if needed.
pub(crate) async fn handle_block_reverts<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    txn: StorageTxn<'_, RW>,
    central_source: Arc<TCentralSource>,
) -> Result<(), StateSyncError> {
    debug!("Handling block reverts.");
    let header_marker = reader.begin_ro_txn()?.get_header_marker()?;

    // Revert last blocks if needed.
    let last_block_in_storage = header_marker.prev();
    // while let Some(block_number) = last_block_in_storage {
    if let Some(block_number) = last_block_in_storage {
        if should_revert_block(reader, central_source, block_number).await? {
            info!("Reverting block {}.", block_number);
            revert_block(txn, block_number)?;
            // last_block_in_storage = block_number.prev();
        }
        // } else {
        //     break;
        // }
    }
    Ok(())
}

// Deletes the block data from the storage, moving it to the ommer tables.
#[allow(clippy::expect_fun_call)]
fn revert_block(mut txn: StorageTxn<'_, RW>, block_number: BlockNumber) -> StateSyncResult {
    // TODO: Modify revert functions so they return the deleted data, and use it for inserting
    // to the ommer tables.
    let header = txn
        .get_block_header(block_number)?
        .expect(format!("Tried to revert a missing header of block {block_number}").as_str());
    let transactions = txn
        .get_block_transactions(block_number)?
        .expect(format!("Tried to revert a missing transactions of block {block_number}").as_str());
    let transaction_outputs = txn.get_block_transaction_outputs(block_number)?.expect(
        format!("Tried to revert a missing transaction outputs of block {block_number}").as_str(),
    );

    // TODO: use iter_events of EventsReader once it supports RW transactions.
    let mut events: Vec<_> = vec![];
    for idx in 0..transactions.len() {
        let tx_idx = TransactionIndex(block_number, TransactionOffsetInBlock(idx));
        events.push(txn.get_transaction_events(tx_idx)?.unwrap_or_default());
    }

    txn = txn
        .revert_header(block_number)?
        .insert_ommer_header(header.block_hash, &header)?
        .revert_body(block_number)?
        .insert_ommer_body(
            header.block_hash,
            &transactions,
            &transaction_outputs,
            events.as_slice(),
        )?;

    let (txn, maybe_deleted_data) = txn.revert_state_diff(block_number)?;
    if let Some((thin_state_diff, declared_classes)) = maybe_deleted_data {
        txn.insert_ommer_state_diff(header.block_hash, &thin_state_diff, &declared_classes)?
            .commit()?;
    } else {
        txn.commit()?;
    }
    Ok(())
}

/// Checks if centrals block hash at the block number is different from ours (or doesn't exist).
/// If so, a revert is required.
async fn should_revert_block<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_number: BlockNumber,
) -> Result<bool, StateSyncError> {
    if let Some(central_block_hash) = central_source.get_block_hash(block_number).await? {
        let storage_block_header = reader.begin_ro_txn()?.get_block_header(block_number)?;

        match storage_block_header {
            Some(block_header) => Ok(block_header.block_hash != central_block_hash),
            None => Ok(false),
        }
    } else {
        // Block number doesn't exist in central, revert.
        Ok(true)
    }
}

pub(crate) async fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
    sender: mpsc::Sender<SyncEvent>,
) -> StateSyncResult {
    let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
    let last_block_number = central_source.get_block_marker().await?;
    if header_marker == last_block_number {
        debug!("Waiting for more blocks.");
        tokio::time::sleep(block_propation_sleep_duration).await;
        return Ok(());
    }

    debug!("Downloading blocks [{} - {}).", header_marker, last_block_number);
    let block_stream = central_source.stream_new_blocks(header_marker, last_block_number).fuse();
    pin_mut!(block_stream);

    while let Some(maybe_block) = block_stream.next().await {
        let (block_number, block) = maybe_block?;
        sender.send(SyncEvent::BlockAvailable { block_number, block }).await?;
    }

    Ok(())
}
