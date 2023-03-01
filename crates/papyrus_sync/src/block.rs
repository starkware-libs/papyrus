use std::sync::Arc;

use futures_util::{pin_mut, StreamExt};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::db::RW;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::{Block, BlockNumber};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct BlockSync<TCentralSource: CentralSourceTrait + Sync + Send> {
    config: SyncConfig,
    central_source: Arc<TCentralSource>,
    reader: StorageReader,
    task: JoinHandle<()>,
    receiver: mpsc::Receiver<BlockSyncData>,
}

#[derive(Debug)]
struct BlockSyncData {
    block_number: BlockNumber,
    block: Block,
}

impl<TCentralSource: CentralSourceTrait + Sync + Send + 'static> BlockSync<TCentralSource> {
    pub fn new(
        config: SyncConfig,
        central_source: Arc<TCentralSource>,
        reader: StorageReader,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let task = run_stream_new_blocks(config, central_source.clone(), reader.clone(), sender);
        BlockSync { config, central_source, reader, task, receiver }
    }

    pub fn step(&mut self, txn: StorageTxn<'_, RW>) -> StateSyncResult {
        match self.receiver.try_recv() {
            Ok(BlockSyncData { block_number, block }) => {
                self.store_block(txn, block_number, block)?;
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
        self.task = run_stream_new_blocks(
            self.config,
            self.central_source.clone(),
            self.reader.clone(),
            sender,
        );
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
            self.restart_task();
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
    sender: mpsc::Sender<BlockSyncData>,
) -> JoinHandle<()> {
    let task = async move {
        if let Err(err) = stream_new_blocks(config, reader, central_source, sender).await {
            warn!("{}", err);
            tokio::time::sleep(config.recoverable_error_sleep_duration).await;
        }
    };

    tokio::spawn(task)
}

async fn stream_new_blocks<TCentralSource: CentralSourceTrait + Sync + Send>(
    config: SyncConfig,
    reader: StorageReader,
    central_source: Arc<TCentralSource>,
    sender: mpsc::Sender<BlockSyncData>,
) -> Result<(), StateSyncError> {
    // try_stream! {
    // loop {
    let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
    let last_block_number = central_source.get_block_marker().await?;
    if header_marker == last_block_number {
        debug!("Waiting for more blocks.");
        tokio::time::sleep(config.block_propagation_sleep_duration).await;
        return Ok(());
    }

    debug!("Downloading blocks [{} - {}).", header_marker, last_block_number);
    let block_stream = central_source.stream_new_blocks(header_marker, last_block_number).fuse();
    pin_mut!(block_stream);

    while let Some(maybe_block) = block_stream.next().await {
        let (block_number, block) = maybe_block?;
        // yield SyncEvent::BlockAvailable { block_number, block };
        sender.try_send(BlockSyncData { block_number, block }).map_err(|_| {
            StateSyncError::SyncInternalError {
                msg: format!(
                    "Problem with sending block {block_number} on the channel of the current task."
                ),
            }
        })?;
    }

    Ok(())
    // }
    // }
}
