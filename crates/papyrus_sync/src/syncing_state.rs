use std::sync::Arc;
use std::time::Duration;

use papyrus_common::{BlockHashAndNumber, SyncStatus, SyncingState};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockNumber};
use tokio::sync::RwLock;
use tracing::debug;

use crate::{get_declared_class_range, CentralSourceTrait, StateSyncError};

// Gets the current and highest block (block hash and number) and updates the syncing state.
pub async fn get_status_and_update_syncing_state<
    TCentralSource: CentralSourceTrait + Sync + Send,
>(
    reader: &StorageReader,
    central_source: &Arc<TCentralSource>,
    shared_syncing_state: Arc<RwLock<SyncingState>>,
    current_block_number: Option<BlockNumber>,
) -> Result<(), StateSyncError> {
    let current_block = get_block_hash_and_number(reader, current_block_number)?;
    let highest_block = get_highest_block(central_source).await?;
    update_syncing_state(shared_syncing_state, current_block, highest_block).await;
    debug!("Syncing state was updated.");
    Ok(())
}

// Updates the shared syncing state with the new status.
pub(crate) async fn update_syncing_state(
    shared_syncing_state: Arc<RwLock<SyncingState>>,
    current_block: BlockHashAndNumber,
    highest_block: BlockHashAndNumber,
) {
    if current_block.block_number >= highest_block.block_number {
        *shared_syncing_state.write().await = SyncingState::Synced;
        return;
    }

    let mut lock = shared_syncing_state.write().await;
    let (starting_block_num, starting_block_hash) = match *lock {
        SyncingState::Synced => (current_block.block_number, current_block.block_hash),
        SyncingState::SyncStatus(sync_status) => {
            (sync_status.starting_block_num, sync_status.starting_block_hash)
        }
    };
    let sync_status = SyncStatus {
        starting_block_hash,
        starting_block_num,
        current_block_hash: current_block.block_hash,
        current_block_num: current_block.block_number,
        highest_block_hash: highest_block.block_hash,
        highest_block_num: highest_block.block_number,
    };
    *lock = SyncingState::SyncStatus(sync_status);
}

// Writes the updated syncing state every `syncing_state_update_interval`.
pub async fn periodically_update_syncing_state<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    syncing_state_update_interval: Duration,
    shared_syncing_state: Arc<RwLock<SyncingState>>,
) {
    let mut interval = tokio::time::interval(syncing_state_update_interval);
    loop {
        interval.tick().await;
        let Ok(current_block_marker) = get_declared_class_range(&reader) else {continue;};
        if get_status_and_update_syncing_state(
            &reader,
            &central_source,
            shared_syncing_state.clone(),
            current_block_marker.0.prev(),
        )
        .await
        .is_err()
        {
            debug!("Syncing state error.");
        }
    }
}

// Gets the latest block from the central source (block hash and number).
async fn get_highest_block<TCentralSource: CentralSourceTrait + Sync + Send>(
    central_source: &Arc<TCentralSource>,
) -> Result<BlockHashAndNumber, StateSyncError> {
    // TODO(yoav): In case of revert after getting the highest_block_number, the actual highest
    // block number might be lower, and this hash will be None.
    let highest_block_number = central_source.get_block_marker().await?.prev();
    let highest_block_hash = match highest_block_number {
        Some(block_number) => central_source.get_block_hash(block_number).await?.unwrap(),
        None => BlockHash::default(),
    };
    let highest_block_number = highest_block_number.unwrap_or(BlockNumber::default());
    Ok(BlockHashAndNumber { block_hash: highest_block_hash, block_number: highest_block_number })
}

// Gets a block from the storage (block hash and number) by a block number.
fn get_block_hash_and_number(
    reader: &StorageReader,
    opt_block_number: Option<BlockNumber>,
) -> Result<BlockHashAndNumber, StateSyncError> {
    let Some(block_number) = opt_block_number else {
        return Ok(BlockHashAndNumber::default())
    };
    let txn = reader.begin_ro_txn()?;
    let Some(block_header) = txn.get_block_header(block_number)? else {
        panic!("Expecting to have header of block {}", block_number)
    };
    Ok(BlockHashAndNumber { block_hash: block_header.block_hash, block_number })
}
