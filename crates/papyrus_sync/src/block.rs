use std::sync::Arc;

use futures_util::{pin_mut, StreamExt};
use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
use papyrus_storage::db::RW;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageTxn, TransactionIndex};
use starknet_api::block::{Block, BlockHeader, BlockNumber};
use starknet_api::transaction::TransactionOffsetInBlock;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig, SyncEvent};

pub struct BlockSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    pub config: SyncConfig,
    pub central_source: Arc<TCentralSource>,
    pub reader: StorageReader,
    pub sender: mpsc::Sender<SyncEvent>,
}

pub async fn run_block_sync<TCentralSource: CentralSourceTrait + Sync + Send>(
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    sender: mpsc::Sender<SyncEvent>,
) {
    let block_sync = BlockSync { config, central_source, reader, sender };
    info!("Block sync started.");
    loop {
        match block_sync.stream_new_blocks().await {
            Err(err) => {
                warn!("{}", err);
                tokio::time::sleep(block_sync.config.recoverable_error_sleep_duration).await;
                continue;
            }
            Ok(()) => continue,
        }
    }
}

pub(crate) fn store_block(
    reader: StorageReader,
    txn: StorageTxn<'_, RW>,
    block_number: BlockNumber,
    block: Block,
) -> StateSyncResult {
    trace!("Block data: {block:#?}");

    if let (true, maybe_parent) = is_reverted(reader, block_number, &block)? {
        if let Some(parent) = maybe_parent {
            info!("Reverting block {}.", block_number);
            revert_block(txn, parent.block_number)?;
        }
        return Ok(());
    }

    if let Ok(txn) = txn.append_header(block_number, &block.header) {
        info!("Storing block {block_number} with hash {}.", block.header.block_hash);
        txn.append_body(block_number, block.body)?.commit()?;
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

impl<TCentralSource: CentralSourceTrait + Sync + Send> BlockSync<TCentralSource> {
    async fn stream_new_blocks(&self) -> StateSyncResult {
        let header_marker = self.reader.begin_ro_txn()?.get_header_marker()?;
        let last_block_number = self.central_source.get_block_marker().await?;
        if header_marker == last_block_number {
            debug!("Waiting for more blocks.");
            tokio::time::sleep(self.config.block_propagation_sleep_duration).await;
            return Ok(());
        }

        debug!("Downloading blocks [{} - {}).", header_marker, last_block_number);
        let block_stream =
            self.central_source.stream_new_blocks(header_marker, last_block_number).fuse();
        pin_mut!(block_stream);

        while let Some(maybe_block) = block_stream.next().await {
            let (block_number, block) = maybe_block?;
            self.sender
                .send(SyncEvent::BlockAvailable { block_number, block: block.clone() })
                .await?;
            if let (true, Some(_)) = is_reverted(self.reader.clone(), block_number, &block)? {
                debug!("Waiting for blocks to revert.");
                tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                break;
            }
        }

        Ok(())
    }
}

// Returns:
// (false, None) - first header.
// (true, Some) - if the parent header exists in storage with different hash.
// (false, Some) - if the parent header exists in storage with the same hash.
// (true, None) - if the parent header is not in storage yet.
fn is_reverted(
    reader: StorageReader,
    block_number: BlockNumber,
    block: &Block,
) -> Result<(bool, Option<BlockHeader>), StateSyncError> {
    let prev_block_number = match block_number.prev() {
        None => return Ok((false, None)),
        Some(bn) => bn,
    };
    let prev_header = reader.begin_ro_txn()?.get_block_header(prev_block_number)?;
    match prev_header {
        Some(prev_header) => {
            Ok((prev_header.block_hash != block.header.parent_hash, Some(prev_header)))
        }
        _ => Ok((true, None)),
    }
}
