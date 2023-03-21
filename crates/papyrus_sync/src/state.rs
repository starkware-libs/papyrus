#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use papyrus_storage::db::RW;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::data::StateDiffSyncData;
use crate::downloads_manager::DownloadsManager;
use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct StateDiffSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    reader: StorageReader,
    task: JoinHandle<()>,
    downloads_manager: DownloadsManager<TCentralSource, StateDiffSyncData>,
    last_state_diff_received: BlockNumber,
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> StateDiffSync<TCentralSource> {
    pub fn new(
        config: SyncConfig,
        central_source: Arc<TCentralSource>,
        reader: StorageReader,
    ) -> Result<Self, StateSyncError> {
        let (sender, receiver) = mpsc::channel(200);
        let state_marker = reader.begin_ro_txn()?.get_state_marker()?;
        let downloads_manager =
            DownloadsManager::new(central_source, 10, 100, receiver, state_marker);
        let task = run_stream_new_state_diffs(config, reader.clone(), sender);
        Ok(StateDiffSync {
            config,
            reader,
            task,
            downloads_manager,
            last_state_diff_received: state_marker,
        })
    }

    pub fn step(&mut self, txn: StorageTxn<'_, RW>) -> StateSyncResult {
        // Check if there was a revert.
        let state_marker = self.reader.begin_ro_txn()?.get_state_marker()?;
        if state_marker < self.last_state_diff_received {
            info!("Restart state diff sync because of block {state_marker}.");
            return self.restart();
        }

        let res = self.downloads_manager.step();
        if let Err(err) = res {
            warn!("{}", err);
            return self.restart();
        }

        if let Some(StateDiffSyncData {
            block_number,
            block_hash,
            state_diff,
            deployed_contract_class_definitions,
        }) = res.unwrap()
        {
            return self.store_state_diff(
                txn,
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            );
        }

        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), StateSyncError> {
        info!("Restarting state diff sync");
        self.task.abort();
        self.downloads_manager.drop();

        let (sender, receiver) = mpsc::channel(200);
        let state_marker = self.reader.begin_ro_txn()?.get_state_marker()?;
        self.last_state_diff_received = state_marker;
        self.downloads_manager.reset(receiver, state_marker);

        self.task = run_stream_new_state_diffs(self.config, self.reader.clone(), sender);
        Ok(())
    }

    fn store_state_diff(
        &mut self,
        txn: StorageTxn<'_, RW>,
        block_number: BlockNumber,
        block_hash: BlockHash,
        mut state_diff: StateDiff,
        deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
    ) -> StateSyncResult {
        trace!("StateDiff data: {state_diff:#?}");

        if self.should_store(block_number, block_hash)? {
            info!("Storing state diff of block {block_number} with hash {block_hash}.");
            sort_state_diff(&mut state_diff);
            txn.append_state_diff(block_number, state_diff, deployed_contract_class_definitions)?
                .commit()?;
            self.last_state_diff_received = block_number;
        } else {
            info!("Restart state diff sync because of block {block_number}.");
            self.restart()?;
        }

        Ok(())
    }

    fn should_store(
        &self,
        block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> Result<bool, StateSyncError> {
        let txn = self.reader.begin_ro_txn()?;
        if let Some(storage_header) = txn.get_block_header(block_number)? {
            if storage_header.block_hash == block_hash {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

fn run_stream_new_state_diffs(
    config: SyncConfig,
    reader: StorageReader,
    sender: mpsc::Sender<BlockNumber>,
) -> JoinHandle<()> {
    let task = async move {
        if let Err(err) = stream_new_state_diffs(reader, sender).await {
            warn!("{}", err);
            tokio::time::sleep(config.recoverable_error_sleep_duration).await;
        }
    };

    tokio::spawn(task)
}

async fn stream_new_state_diffs(
    reader: StorageReader,
    sender: mpsc::Sender<BlockNumber>,
) -> Result<(), StateSyncError> {
    let mut last_sent = reader.begin_ro_txn()?.get_state_marker()?;
    loop {
        let txn = reader.begin_ro_txn()?;
        let state_marker = txn.get_state_marker()?;
        let last_block_number = txn.get_header_marker()?;
        drop(txn);
        if state_marker == last_block_number {
            trace!("Stored all state diffs - waiting for the block chain to advance.");
            tokio::time::sleep(Duration::from_millis(10)).await; // TODO(anatg): Add to config file.
            continue;
        }

        if last_sent >= last_block_number {
            trace!("Sent last range update - waiting for the block chain to advance.");
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }

        debug!("Sending upto {}.", last_block_number);
        sender.send(last_block_number).await.map_err(|e| StateSyncError::Channel {
            msg: format!("Problem with sending upto {last_block_number}: {e}."),
        })?;
        last_sent = last_block_number;
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
