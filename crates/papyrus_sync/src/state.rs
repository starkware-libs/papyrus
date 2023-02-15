#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use indexmap::IndexMap;
use papyrus_storage::db::RW;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, info, trace};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, SyncEvent};

pub fn sync_state_while_ok<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
    state_sync_idle: bool,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    try_stream! {
        loop {
            debug!("Started sync_state_while_ok.");
            if state_sync_idle {
                debug!("Waiting because state sync was idle.");
                tokio::time::sleep(block_propagation_sleep_duration).await;
            }

            let txn = reader.begin_ro_txn()?;
            let state_marker = txn.get_state_marker()?;
            let last_block_number = txn.get_header_marker()?;
            if state_marker == last_block_number {
                debug!("Waiting for block chain to advance.");
                tokio::time::sleep(block_propagation_sleep_duration).await;
                continue;
            }

            info!("Downloading state diffs [{state_marker} - {last_block_number}).");
            let state_diff_stream =
                central_source.stream_state_updates(state_marker, last_block_number).fuse();
            for await maybe_state_diff in state_diff_stream {
                let (block_number, block_hash, mut state_diff, deployed_contract_class_definitions) =
                    maybe_state_diff?;
                sort_state_diff(&mut state_diff);
                yield SyncEvent::StateDiffAvailable { block_number, block_hash, state_diff, deployed_contract_class_definitions };
            }
        }
    }
}

pub fn process_state_diff_event(
    mut txn: StorageTxn<'_, RW>,
    block_number: BlockNumber,
    block_hash: BlockHash,
    state_diff: StateDiff,
    deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
) -> Result<(StorageTxn<'_, RW>, bool), StateSyncError> {
    let storage_header = txn.get_block_header(block_number)?;
    if storage_header.is_some() && storage_header.unwrap().block_hash == block_hash {
        info!("Storing state diff of block {block_number} with hash {block_hash}.");
        trace!("State diff: {state_diff:#?}");
        txn =
            txn.append_state_diff(block_number, state_diff, deployed_contract_class_definitions)?;
        info!("Updated state upto block number {block_number}.");
        return Ok((txn, false));
    }

    Ok((txn, true))
}

fn sort_state_diff(diff: &mut StateDiff) {
    diff.declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
    }
}
