#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;
use std::time::Duration;

use futures::pin_mut;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageWriter};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::{error, info};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult};

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

        info!("Downloading state diffs [{} - {}).", state_marker, last_block_number);
        let state_diff_stream =
            central_source.stream_state_updates(state_marker, last_block_number).fuse();
        pin_mut!(state_diff_stream);

        while let Some(maybe_state_diff) = state_diff_stream.next().await {
            match maybe_state_diff {
                Ok((
                    block_number,
                    block_hash,
                    mut state_diff,
                    deployed_contract_class_definitions,
                )) => {
                    sort_state_diff(&mut state_diff);
                    store_state_diff(
                        writer.clone(),
                        block_number,
                        block_hash,
                        state_diff,
                        deployed_contract_class_definitions,
                    )
                    .await?;
                }
                Err(err) => {
                    return Err(StateSyncError::CentralSourceError(err));
                }
            }
        }
    }
}

pub async fn store_state_diff(
    writer: Arc<Mutex<StorageWriter>>,
    block_number: BlockNumber,
    block_hash: BlockHash,
    state_diff: StateDiff,
    deployed_contract_class_definitions: Vec<(ClassHash, ContractClass)>,
) -> StateSyncResult {
    let mut locked_writer = writer.lock().await;
    let txn = locked_writer.begin_rw_txn()?;

    let storage_header = txn.get_block_header(block_number)?;
    if storage_header.is_some() && storage_header.unwrap().block_hash == block_hash {
        info!("Storing state diff of block {}.", block_number);
        txn.append_state_diff(block_number, state_diff, deployed_contract_class_definitions)?
            .commit()?;
        // Info the user on syncing the block once all the data is stored.
        info!("Added block {} with hash {}.", block_number, block_hash);
    }

    Ok(())
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
