use std::sync::Arc;

use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::db::RW;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::{Block, BlockNumber};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::downloads_manager::{BlockSyncData, DownloadsManager};
use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct BlockSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    task: JoinHandle<()>,
    downloads_manager: DownloadsManager<TCentralSource, BlockSyncData>,
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> BlockSync<TCentralSource> {
    pub fn new(
        config: SyncConfig,
        central_source: Arc<TCentralSource>,
        reader: StorageReader,
    ) -> Result<Self, StateSyncError> {
        let (sender, receiver) = mpsc::channel(200);
        let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
        let downloads_manager =
            DownloadsManager::new(central_source.clone(), 10, 100, receiver, header_marker);
        let task = run_stream_new_blocks(config, central_source.clone(), reader.clone(), sender);
        Ok(BlockSync { config, central_source, reader, task, downloads_manager })
    }

    pub fn step(&mut self, txn: StorageTxn<'_, RW>) -> StateSyncResult {
        let res = self.downloads_manager.step();
        if let Err(err) = res {
            warn!("{}", err);
            return self.restart();
        }

        if let Some(BlockSyncData { block_number, block }) = res.unwrap() {
            return self.store_block(txn, block_number, block);
        }

        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), StateSyncError> {
        info!("Restarting block sync");
        self.task.abort();
        self.downloads_manager.drop();

        let (sender, receiver) = mpsc::channel(200);
        let header_marker = self.reader.begin_ro_txn()?.get_header_marker()?;
        self.downloads_manager.reset(receiver, header_marker);

        self.task = run_stream_new_blocks(
            self.config,
            self.central_source.clone(),
            self.reader.clone(),
            sender,
        );
        Ok(())
    }

    fn store_block(
        &mut self,
        txn: StorageTxn<'_, RW>,
        block_number: BlockNumber,
        block: Block,
    ) -> StateSyncResult {
        trace!("Block data: {block:#?}");

        if self.should_store(block_number, &block)? {
            info!("Storing block {block_number} with hash {}.", block.header.block_hash);
            txn.append_header(block_number, &block.header)?
                .append_body(block_number, block.body)?
                .commit()?;
        } else {
            info!("Reverting block {}.", block_number.prev().unwrap());
            revert_block(txn, block_number.prev().unwrap())?;
            self.restart()?;
        }

        Ok(())
    }

    fn should_store(
        &self,
        block_number: BlockNumber,
        block: &Block,
    ) -> Result<bool, StateSyncError> {
        let prev_block_number = match block_number.prev() {
            None => return Ok(true),
            Some(bn) => bn,
        };

        if let Some(prev_header) =
            self.reader.begin_ro_txn()?.get_block_header(prev_block_number)?
        {
            // Compares the block's parent hash to the stored block.
            if prev_header.block_hash == block.header.parent_hash {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

// Deletes the block data from the storage, moving it to the ommer tables.
#[allow(clippy::expect_fun_call)]
fn revert_block(mut txn: StorageTxn<'_, RW>, block_number: BlockNumber) -> StateSyncResult {
    let res = txn.revert_header(block_number)?;
    txn = res.0;
    if let Some(header) = res.1 {
        txn = txn.insert_ommer_header(header.block_hash, &header)?;
        let res = txn.revert_body(block_number)?;
        txn = res.0;
        if let Some((transactions, transaction_outputs, events)) = res.1 {
            txn = txn.insert_ommer_body(
                header.block_hash,
                &transactions,
                &transaction_outputs,
                events.as_slice(),
            )?;
        }

        let res = txn.revert_state_diff(block_number)?;
        txn = res.0;
        if let Some((thin_state_diff, declared_classes)) = res.1 {
            txn = txn.insert_ommer_state_diff(
                header.block_hash,
                &thin_state_diff,
                &declared_classes,
            )?;
        }
    }
    txn.commit()?;
    Ok(())
}

fn run_stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    sender: mpsc::Sender<BlockNumber>,
) -> JoinHandle<()> {
    let task = async move {
        if let Err(err) = stream_new_blocks(config, reader, central_source, sender).await {
            warn!("{}", err);
            tokio::time::sleep(config.recoverable_error_sleep_duration).await;
        }
    };

    tokio::spawn(task)
}

async fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    config: SyncConfig,
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    sender: mpsc::Sender<BlockNumber>,
) -> Result<(), StateSyncError> {
    let mut last_sent = reader.begin_ro_txn()?.get_header_marker()?;
    loop {
        let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
        let last_block_number = central_source.get_block_marker().await?;
        if header_marker == last_block_number {
            debug!("Stored all blocks - waiting for more blocks.");
            tokio::time::sleep(config.block_propagation_sleep_duration).await;
            continue;
        }

        if last_sent >= last_block_number {
            debug!("Sent last range update - waiting for more blocks.");
            tokio::time::sleep(config.block_propagation_sleep_duration).await;
            continue;
        }

        debug!("Sending upto {}.", last_block_number);
        sender.send(last_block_number).await.map_err(|e| StateSyncError::Channel {
            msg: format!("Problem with sending upto {last_block_number}: {e}."),
        })?;
        last_sent = last_block_number;
    }
}
