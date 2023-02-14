#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures_util::{pin_mut, Stream, StreamExt};
use indexmap::IndexMap;
use papyrus_storage::db::RW;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::ommer::{OmmerStorageReader, OmmerStorageWriter};
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tracing::{debug, info, trace};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncEvent};

pub(crate) fn store_state_diff(
    reader: StorageReader,
    txn: StorageTxn<'_, RW>,
    block_number: BlockNumber,
    block_hash: BlockHash,
    state_diff: StateDiff,
    deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
) -> StateSyncResult {
    if !is_reverted_state_diff(reader, block_number, block_hash)? {
        debug!("Storing state diff of block {block_number} with hash {block_hash}.");
        trace!("StateDiff data: {state_diff:#?}");
        txn.append_state_diff(block_number, state_diff, deployed_contract_class_definitions)?
            .commit()?;

        // Info the user on syncing the block once all the data is stored.
        info!("Added block {} with hash {}.", block_number, block_hash);
    } else {
        debug!("Storing ommer state diff of block {} with hash {:?}.", block_number, block_hash);
        txn.insert_ommer_state_diff(
            block_hash,
            &state_diff.into(),
            &deployed_contract_class_definitions,
        )?
        .commit()?;
    }
    Ok(())
}

fn is_reverted_state_diff(
    reader: StorageReader,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> Result<bool, StateSyncError> {
    let txn = reader.begin_ro_txn()?;
    let storage_header = txn.get_block_header(block_number)?;
    match storage_header {
        Some(storage_header) if storage_header.block_hash == block_hash => Ok(false),
        _ => {
            // No matching header, check in the ommer headers.
            match txn.get_ommer_header(block_hash)? {
                Some(_) => Ok(true),
                None => {
                    Err(StateSyncError::StateDiffWithoutMatchingHeader { block_number, block_hash })
                }
            }
        }
    }
}

pub(crate) fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    block_propation_sleep_duration: Duration,
) -> impl Stream<Item = Result<SyncEvent, StateSyncError>> {
    stream! {
        loop {
            let txn = reader.begin_ro_txn()?;
            let state_marker = txn.get_state_marker()?;
            let last_block_number = txn.get_header_marker()?;
            drop(txn);
            debug!("Downloading state diffs [{} - {}).", state_marker, last_block_number);
            if state_marker == last_block_number {
                tokio::time::sleep(block_propation_sleep_duration).await;
                continue;
            }
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
                        yield Ok(SyncEvent::StateDiffAvailable {
                            block_number,
                            block_hash,
                            state_diff,
                            deployed_contract_class_definitions,
                        });
                    }
                    Err(err) => {
                        yield Err(StateSyncError::CentralSourceError(err));
                        break;
                    }
                }
            }
        }
    }
}

pub(crate) fn sort_state_diff(diff: &mut StateDiff) {
    diff.declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
    }
}
