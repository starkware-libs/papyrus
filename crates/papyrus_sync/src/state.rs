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
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct StateDiffSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    task: JoinHandle<()>,
    receiver: mpsc::Receiver<StateDiffSyncData>,
}

#[derive(Debug)]
struct StateDiffSyncData {
    block_number: BlockNumber,
    block_hash: BlockHash,
    state_diff: StateDiff,
    // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
    // Class definitions of deployed contracts with classes that were not declared in this
    // state diff.
    deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> StateDiffSync<TCentralSource> {
    pub fn new(
        config: SyncConfig,
        central_source: Arc<TCentralSource>,
        reader: StorageReader,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let task =
            run_stream_new_state_diffs(config, central_source.clone(), reader.clone(), sender);
        StateDiffSync { config, central_source, reader, task, receiver }
    }

    pub fn step(&mut self, txn: StorageTxn<'_, RW>) -> StateSyncResult {
        match self.receiver.try_recv() {
            Ok(StateDiffSyncData {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            }) => {
                self.store_state_diff(
                    txn,
                    block_number,
                    block_hash,
                    state_diff,
                    deployed_contract_class_definitions,
                )?;
            }
            Err(TryRecvError::Empty) => {
                debug!("Empty channel - the task is waiting.");
            }
            Err(TryRecvError::Disconnected) => {
                debug!("Disconnected channel - the task is finished. Restart task.");
                self.restart_task();
            }
        }

        Ok(())
    }

    fn restart_task(&mut self) {
        self.task.abort();
        self.receiver.close();

        let (sender, receiver) = mpsc::channel(100);
        self.receiver = receiver;
        self.task = run_stream_new_state_diffs(
            self.config,
            self.central_source.clone(),
            self.reader.clone(),
            sender,
        );
    }

    fn store_state_diff(
        &mut self,
        txn: StorageTxn<'_, RW>,
        block_number: BlockNumber,
        block_hash: BlockHash,
        state_diff: StateDiff,
        deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
    ) -> StateSyncResult {
        match self.is_reverted_state_diff(block_number, block_hash) {
            Ok(false) => {
                info!("Storing state diff of block {block_number} with hash {block_hash}.");
                trace!("StateDiff data: {state_diff:#?}");
                txn.append_state_diff(
                    block_number,
                    state_diff,
                    deployed_contract_class_definitions,
                )?
                .commit()?;
            }
            Ok(true) => {
                debug!(
                    "Storing ommer state diff of block {} with hash {:?}.",
                    block_number, block_hash
                );
                txn.insert_ommer_state_diff(
                    block_hash,
                    &state_diff.into(),
                    &deployed_contract_class_definitions,
                )?
                .commit()?;

                debug!("Restart current task because of block {block_number}.");
                self.restart_task();
            }
            Err(StateSyncError::StateDiffWithoutMatchingHeader { block_number, block_hash: _ }) => {
                debug!("Restart current task because of block {block_number}.");
                self.restart_task();
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    fn is_reverted_state_diff(
        &self,
        block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> Result<bool, StateSyncError> {
        let txn = self.reader.begin_ro_txn()?;
        let storage_header = txn.get_block_header(block_number)?;
        match storage_header {
            Some(storage_header) if storage_header.block_hash == block_hash => Ok(false),
            _ => {
                // No matching header, check in the ommer headers.
                match txn.get_ommer_header(block_hash)? {
                    Some(_) => Ok(true),
                    None => Err(StateSyncError::StateDiffWithoutMatchingHeader {
                        block_number,
                        block_hash,
                    }),
                }
            }
        }
    }
}

fn run_stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    sender: mpsc::Sender<StateDiffSyncData>,
) -> JoinHandle<()> {
    let task = async move {
        if let Err(err) = stream_new_state_diffs(reader, central_source, sender).await {
            warn!("{}", err);
            tokio::time::sleep(config.recoverable_error_sleep_duration).await;
        }
    };

    tokio::spawn(task)
}

async fn stream_new_state_diffs<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    sender: mpsc::Sender<StateDiffSyncData>,
) -> Result<(), StateSyncError> {
    // try_stream! {
    // loop {
    let txn = reader.begin_ro_txn()?;
    let state_marker = txn.get_state_marker()?;
    let last_block_number = txn.get_header_marker()?;
    drop(txn);
    if state_marker == last_block_number {
        debug!("Waiting for the block chain to advance.");
        // tokio::time::sleep(block_propagation_sleep_duration).await;
        return Ok(());
    }

    debug!("Downloading state diffs [{} - {}).", state_marker, last_block_number);
    let state_diff_stream =
        central_source.stream_state_updates(state_marker, last_block_number).fuse();
    pin_mut!(state_diff_stream);

    while let Some(maybe_state_diff) = state_diff_stream.next().await {
        let (block_number, block_hash, mut state_diff, deployed_contract_class_definitions) =
            maybe_state_diff?;
        sort_state_diff(&mut state_diff);
        // yield SyncEvent::StateDiffAvailable {
        //     block_number,
        //     block_hash,
        //     state_diff,
        //     deployed_contract_class_definitions,
        // };
        sender
            .try_send(StateDiffSyncData {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            })
            .map_err(|_| StateSyncError::SyncInternalError {
                msg: format!(
                    "Problem with sending state diff of block {block_number} on the channel of \
                     the current task."
                ),
            })?;
    }

    Ok(())
    // }
    // }
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
