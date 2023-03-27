use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_storage::db::{RO, RW};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::data::SyncDataTrait;
use crate::downloads_manager::DownloadsManager;
use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct SingleDataTypeSync<T, D, S>
where
    T: CentralSourceTrait + Sync + Send,
    D: SyncDataTrait,
    S: SyncExtensionTrait<T, D>,
{
    sync_ext: PhantomData<S>,
    config: SyncConfig,
    source: Arc<T>,
    reader: StorageReader,
    task: JoinHandle<()>,
    downloads_manager: DownloadsManager<T, D>,
    last_received: BlockNumber,
}

impl<T, D, S> SingleDataTypeSync<T, D, S>
where
    T: CentralSourceTrait + Sync + Send + 'static,
    D: SyncDataTrait,
    S: SyncExtensionTrait<T, D>,
{
    pub fn new(
        config: SyncConfig,
        source: Arc<T>,
        reader: StorageReader,
    ) -> Result<Self, StateSyncError> {
        let (sender, receiver) = mpsc::channel(10);
        let marker = S::get_from(&reader)?;
        let downloads_manager = DownloadsManager::new(
            source.clone(),
            config.downloads_manager_max_active_tasks,
            config.downloads_manager_max_range_per_task,
            receiver,
            marker,
        );
        let task = Self::run_update_range(config, source.clone(), reader.clone(), sender);
        Ok(Self {
            sync_ext: PhantomData::<S>,
            config,
            source,
            reader,
            task,
            downloads_manager,
            last_received: marker,
        })
    }

    pub fn step(&mut self, txn: StorageTxn<'_, RW>) -> StateSyncResult {
        // Check if there was a revert.
        let marker = S::get_from(&self.reader)?;
        if marker < self.last_received {
            info!("Restart {:?} sync because of block {marker}.", D::r#type());
            return self.restart();
        }

        let res = self.downloads_manager.step();
        if let Err(err) = res {
            warn!("{}", err);
            return self.restart();
        }

        if let Some(data) = res.unwrap() {
            trace!("Storing data: {data:#?}");
            let block_number = data.block_number();
            let block_hash = data.block_hash();
            if S::should_store(&self.reader, &data)? {
                info!(
                    "Storing {:?} data of block {block_number} with hash {block_hash}.",
                    D::r#type()
                );
                S::store(txn, data)?;
                self.last_received = block_number;
            } else {
                S::revert_if_necessary(txn, &data)?;
                self.restart()?;
            }
        }

        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), StateSyncError> {
        info!("Restarting {:?} sync", D::r#type());
        self.task.abort();
        self.downloads_manager.drop();

        let (sender, receiver) = mpsc::channel(200);
        let marker = S::get_from(&self.reader)?;
        self.last_received = marker;
        self.downloads_manager.reset(receiver, marker);

        self.task =
            Self::run_update_range(self.config, self.source.clone(), self.reader.clone(), sender);
        Ok(())
    }

    fn run_update_range(
        config: SyncConfig,
        source: Arc<T>,
        reader: StorageReader,
        sender: mpsc::Sender<BlockNumber>,
    ) -> JoinHandle<()> {
        let task = async move {
            if let Err(err) = Self::update_range(config, source, reader, sender).await {
                warn!("{}", err);
                tokio::time::sleep(config.recoverable_error_sleep_duration).await;
            }
        };

        tokio::spawn(task)
    }

    async fn update_range(
        config: SyncConfig,
        source: Arc<T>,
        reader: StorageReader,
        sender: mpsc::Sender<BlockNumber>,
    ) -> Result<(), StateSyncError> {
        let (mut last_sent, _) = S::get_range(reader.begin_ro_txn()?, source.clone()).await?;
        loop {
            let (marker, last_block_number) =
                S::get_range(reader.begin_ro_txn()?, source.clone()).await?;

            if marker == last_block_number {
                trace!("[{:?}]: Stored all data - waiting for more blocks.", D::r#type());
                tokio::time::sleep(S::get_sleep_duration(config)).await;
                continue;
            }

            if last_sent >= last_block_number {
                trace!("[{:?}]: Sent last range update - waiting for more blocks.", D::r#type());
                tokio::time::sleep(S::get_sleep_duration(config)).await;
                continue;
            }

            debug!("Sending upto {last_block_number} for {:?} sync.", D::r#type());
            sender.send(last_block_number).await.map_err(|e| StateSyncError::Channel {
                msg: format!("Problem with sending upto {last_block_number}: {e}."),
            })?;
            last_sent = last_block_number;
        }
    }
}

#[async_trait]
pub trait SyncExtensionTrait<T: CentralSourceTrait + Sync + Send, D: SyncDataTrait> {
    fn get_from(reader: &StorageReader) -> Result<BlockNumber, StateSyncError>;
    fn store(txn: StorageTxn<'_, RW>, data: D) -> StateSyncResult;
    fn should_store(reader: &StorageReader, data: &D) -> Result<bool, StateSyncError>;
    fn revert_if_necessary(txn: StorageTxn<'_, RW>, data: &D) -> StateSyncResult;
    async fn get_range(
        txn: StorageTxn<'_, RO>,
        source: Arc<T>,
    ) -> Result<(BlockNumber, BlockNumber), StateSyncError>;
    fn get_sleep_duration(config: SyncConfig) -> Duration;
}
