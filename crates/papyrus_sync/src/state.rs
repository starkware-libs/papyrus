#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;

use futures_util::{pin_mut, StreamExt};
use indexmap::IndexMap;
use papyrus_storage::db::RW;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::ommer::{OmmerStorageReader, OmmerStorageWriter};
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio::sync::mpsc;
use tracing::{debug, info, trace};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig, SyncEvent};

pub struct StateDiffSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    pub config: SyncConfig,
    pub central_source: Arc<TCentralSource>,
    pub reader: StorageReader,
    pub sender: mpsc::Sender<SyncEvent>,
}

pub async fn run_state_diff_sync<TCentralSource: CentralSourceTrait + Sync + Send>(
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    sender: mpsc::Sender<SyncEvent>,
) -> StateSyncResult {
    let state_sync = StateDiffSync { config, central_source, reader, sender };
    state_sync.stream_new_state_diffs().await
}

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

impl<TCentralSource: CentralSourceTrait + Sync + Send> StateDiffSync<TCentralSource> {
    async fn stream_new_state_diffs(&self) -> StateSyncResult {
        let txn = self.reader.begin_ro_txn()?;
        let state_marker = txn.get_state_marker()?;
        let last_block_number = txn.get_header_marker()?;
        drop(txn);
        if state_marker == last_block_number {
            debug!("Waiting for the block chain to advance.");
            tokio::time::sleep(self.config.block_propagation_sleep_duration).await;
            return Ok(());
        }

        debug!("Downloading state diffs [{} - {}).", state_marker, last_block_number);
        let state_diff_stream =
            self.central_source.stream_state_updates(state_marker, last_block_number).fuse();
        pin_mut!(state_diff_stream);

        while let Some(maybe_state_diff) = state_diff_stream.next().await {
            let (block_number, block_hash, mut state_diff, deployed_contract_class_definitions) =
                maybe_state_diff?;
            sort_state_diff(&mut state_diff);
            self.sender
                .send(SyncEvent::StateDiffAvailable {
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                })
                .await?;
            if is_reverted_state_diff_if_exists(self.reader.clone(), block_number, block_hash)? {
                debug!("Waiting for blocks to revert.");
                tokio::time::sleep(self.config.recoverable_error_sleep_duration).await;
                break;
            }
        }

        Ok(())
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

fn is_reverted_state_diff_if_exists(
    reader: StorageReader,
    block_number: BlockNumber,
    block_hash: BlockHash,
) -> Result<bool, StateSyncError> {
    let txn = reader.begin_ro_txn()?;
    let storage_header = txn.get_block_header(block_number)?;
    match storage_header {
        Some(storage_header) if storage_header.block_hash != block_hash => {
            debug!(
                "Detected a possible revert while processing state diff of block {block_number}."
            );
            Ok(true)
        }
        _ => Ok(false),
    }
}
