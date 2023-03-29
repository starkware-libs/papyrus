use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_storage::db::RW;
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace, warn};

use crate::data::{SyncData, SyncDataTrait};
use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct SingleDataTypeSync<T, D, S>
where
    T: CentralSourceTrait + Sync + Send,
    D: SyncDataTrait,
    S: SyncExtensionTrait<T, D>,
{
    sync_ext: PhantomData<(S, D)>,
    config: SyncConfig,
    source: Arc<T>,
    reader: StorageReader,
    task: JoinHandle<()>,
    receiver: mpsc::Receiver<SyncData>,
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
        let (sender, receiver) = mpsc::channel(100);
        let marker = S::get_from(&reader)?;
        let task = Self::run_stream_new_data(config, source.clone(), reader.clone(), sender);
        Ok(Self {
            sync_ext: PhantomData::<(S, D)>,
            config,
            source,
            reader,
            task,
            receiver,
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

        match self.receiver.try_recv() {
            Ok(sync_data) => {
                let data = D::try_from(sync_data)?;
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
            Err(mpsc::error::TryRecvError::Empty) => {
                debug!("[{:?} sync]: Empty channel - the task is waiting.", D::r#type());
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                warn!("[{:?} sync]: Disconnected channel - task failed.", D::r#type());
                self.restart()?;
            }
        }

        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), StateSyncError> {
        info!("Restarting {:?} sync", D::r#type());
        self.task.abort();
        self.receiver.close();

        let (sender, receiver) = mpsc::channel(200);
        let marker = S::get_from(&self.reader)?;
        self.last_received = marker;
        self.receiver = receiver;

        self.task = Self::run_stream_new_data(
            self.config,
            self.source.clone(),
            self.reader.clone(),
            sender,
        );
        Ok(())
    }

    fn run_stream_new_data(
        config: SyncConfig,
        source: Arc<T>,
        reader: StorageReader,
        sender: mpsc::Sender<SyncData>,
    ) -> JoinHandle<()> {
        let task = async move {
            if let Err(err) = Self::stream_new_data(config, source, reader, sender).await {
                warn!("{}", err);
                tokio::time::sleep(config.recoverable_error_sleep_duration).await;
            }
        };

        tokio::spawn(task)
    }

    async fn stream_new_data(
        config: SyncConfig,
        source: Arc<T>,
        reader: StorageReader,
        sender: mpsc::Sender<SyncData>,
    ) -> Result<(), StateSyncError> {
        let (mut last_downloaded, _) = S::get_range(reader.clone(), source.clone()).await?;
        let sender_ref = Arc::new(sender);
        loop {
            let (marker, last_block_number) = S::get_range(reader.clone(), source.clone()).await?;

            if marker == last_block_number {
                debug!("[{:?}]: Stored all data - waiting for more data.", D::r#type());
                tokio::time::sleep(S::get_sleep_duration(config)).await;
                continue;
            }

            if last_downloaded >= last_block_number {
                debug!("[{:?}]: Downloaded all data - waiting for more data.", D::r#type());
                tokio::time::sleep(S::get_sleep_duration(config)).await;
                continue;
            }

            D::download(source.clone(), sender_ref.clone(), last_downloaded, last_block_number)
                .await?;
            last_downloaded = last_block_number;
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
        reader: StorageReader,
        source: Arc<T>,
    ) -> Result<(BlockNumber, BlockNumber), StateSyncError>;
    fn get_sleep_duration(config: SyncConfig) -> Duration;
}
