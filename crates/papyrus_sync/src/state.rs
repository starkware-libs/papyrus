#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;
use std::time::Duration;

use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageWriter};
use starknet_api::state::StateDiff;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::info;

use crate::sources::CentralSourceTrait;
use crate::StateSyncResult;

pub async fn sync_state_while_ok<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    writer: Arc<Mutex<StorageWriter>>,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
) -> StateSyncResult {
    loop {
        let txn = reader.begin_ro_txn()?;
        let state_marker = txn.get_state_marker()?;
        let last_block_number = txn.get_header_marker()?;
        if state_marker == last_block_number {
            tokio::time::sleep(block_propagation_sleep_duration).await;
            continue;
        }

        info!("Downloading state diffs [{state_marker} - {last_block_number}).");
        let mut state_diff_stream =
            central_source.stream_state_updates(state_marker, last_block_number).fuse();

        while let Some(maybe_state_diff) = state_diff_stream.next().await {
            let (block_number, block_hash, mut state_diff, deployed_contract_class_definitions) =
                maybe_state_diff?;
            sort_state_diff(&mut state_diff);

            let mut locked_writer = writer.lock().await;
            let txn = locked_writer.begin_rw_txn()?;

            let storage_header = txn.get_block_header(block_number)?;
            if storage_header.is_some() && storage_header.unwrap().block_hash == block_hash {
                info!("Storing state diff of block {block_number}.");
                txn.append_state_diff(
                    block_number,
                    state_diff,
                    deployed_contract_class_definitions,
                )?
                .commit()?;
                info!("Updated state upto block number {block_number}.");
            }
        }
    }
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
