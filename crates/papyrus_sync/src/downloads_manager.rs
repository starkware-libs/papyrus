use std::cmp::min;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use futures::pin_mut;
use futures::stream::StreamExt;
use indexmap::IndexMap;
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{ContractClass, StateDiff};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};

use crate::sources::{CentralError, CentralSourceTrait};

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and sends data to the sync.
pub struct DownloadsManager<T, D>
where
    T: CentralSourceTrait + Sync + Send,
    D: SyncDataTrait,
{
    tasks: Vec<Task<T, D>>,
    source: Arc<T>,
    max_active_tasks: usize, // TODO(anatg): Add to config file.
    max_range_per_task: u64, // TODO(anatg): Add to config file.
    receiver: mpsc::Receiver<BlockNumber>,
    from: BlockNumber,
    upto: Option<BlockNumber>,
    last_read: Option<BlockNumber>,
}

impl<T, D> DownloadsManager<T, D>
where
    T: CentralSourceTrait + Sync + Send + 'static,
    D: SyncDataTrait,
{
    pub fn new(
        source: Arc<T>,
        max_active_tasks: usize,
        max_range_per_task: u64,
        receiver: mpsc::Receiver<BlockNumber>,
        from: BlockNumber,
    ) -> Self {
        Self {
            tasks: Vec::new(),
            source,
            max_active_tasks,
            max_range_per_task,
            receiver,
            from,
            upto: None,
            last_read: None,
        }
    }

    pub fn reset(&mut self, receiver: mpsc::Receiver<BlockNumber>, from: BlockNumber) {
        self.tasks = Vec::new();
        self.receiver = receiver;
        self.from = from;
        self.upto = None;
    }

    pub fn drop(&mut self) {
        for task in &mut self.tasks {
            task.handle.abort();
            task.receiver.close();
        }
        self.last_read = None;
        self.receiver.close();
    }

    pub fn step(&mut self) -> Result<Option<D>, DownloadsManagerError> {
        self.update_range()?;
        self.update_tasks()?;
        self.get_data()
    }

    fn get_data(&mut self) -> Result<Option<D>, DownloadsManagerError> {
        loop {
            match self.tasks.first_mut() {
                Some(current_task) => match current_task.receiver.try_recv() {
                    Ok(sync_data) => {
                        let data = D::try_from(sync_data)?;
                        self.last_read = Some(data.block_number());
                        trace!("[{:?}]: Sending data {data:?}", D::r#type());
                        return Ok(Some(data));
                    }
                    Err(TryRecvError::Empty) => {
                        if current_task.done_reading(self.last_read) {
                            current_task.receiver.close();
                            self.tasks.remove(0);
                            continue;
                        }
                        trace!("[{:?}]: Current task has no data yet.", D::r#type());
                        return Ok(None);
                    }
                    Err(TryRecvError::Disconnected) => {
                        if current_task.done_reading(self.last_read) {
                            current_task.receiver.close();
                            self.tasks.remove(0);
                            continue;
                        }
                        return Err(DownloadsManagerError::TaskFailed);
                    }
                },
                None => {
                    if self.done_reading() {
                        return Ok(None);
                    }
                    return Err(DownloadsManagerError::NoTask);
                }
            }
        }
    }

    fn update_range(&mut self) -> Result<(), DownloadsManagerError> {
        while let Ok(upto) = self.receiver.try_recv() {
            debug!("[{:?}]: Received upto {upto}.", D::r#type());
            if self.upto.is_some() && self.upto.unwrap() >= upto {
                return Err(DownloadsManagerError::BadRange {
                    from: self.from,
                    upto,
                    curr_from: self.from,
                    curr_upto: self.upto.unwrap(),
                });
            }

            debug!("[{:?}]: Updating upto {upto}.", D::r#type());
            self.upto = Some(upto);
        }
        Ok(())
    }

    fn update_tasks(&mut self) -> Result<(), DownloadsManagerError> {
        if self.upto.is_none() {
            debug!("[{:?}]: No range was set.", D::r#type());
            return Ok(());
        }
        let upto = self.upto.unwrap();

        // Find where to start the next task from.
        let mut from = if let Some(task) = self.tasks.last() {
            task.upto
        } else if let Some(num) = self.last_read {
            num.next()
        } else {
            self.from
        };

        if from != self.from && from == upto {
            debug!("[{:?}]: Done creating tasks for the given range.", D::r#type());
            return Ok(());
        }

        // Check how many tasks are still active.
        let active_tasks: Vec<_> = self.tasks.iter().filter(|t| !t.handle.is_finished()).collect();
        trace!(
            "[{:?}]: There are {} tasks, {} active tasks.",
            D::r#type(),
            self.tasks.len(),
            active_tasks.len()
        );

        // Create new tasks.
        for _i in 0..self.max_active_tasks - active_tasks.len() {
            let task_upto = BlockNumber(min(from.0 + self.max_range_per_task, upto.0));
            debug!("[{:?}]: Creating task [{}, {}).", D::r#type(), from, task_upto);
            let task = Task::new(self.source.clone(), from, task_upto);
            self.tasks.push(task);
            if task_upto == upto {
                break;
            }
            from = task_upto;
        }

        Ok(())
    }

    fn done_reading(&self) -> bool {
        if self.upto.is_none()
            || (self.last_read.is_some() && self.last_read.unwrap().next() == self.upto.unwrap())
        {
            debug!("[{:?}]: Done reading the given range.", D::r#type());
            return true;
        }
        false
    }
}

struct Task<T: CentralSourceTrait + Sync + Send, D: SyncDataTrait> {
    r#type: PhantomData<(T, D)>,
    handle: JoinHandle<()>,
    receiver: mpsc::Receiver<SyncData>,
    from: BlockNumber,
    upto: BlockNumber,
}

impl<T: CentralSourceTrait + Sync + Send + 'static, D: SyncDataTrait> Task<T, D> {
    fn new(source: Arc<T>, from: BlockNumber, upto: BlockNumber) -> Self {
        let (sender, receiver) = mpsc::channel(200);
        let data_type = D::r#type();
        let download = async move {
            if let Err(err) = match data_type {
                SyncDataType::Block => download_blocks(source, sender, from, upto).await,
                SyncDataType::StateDiff => download_state_diffs(source, sender, from, upto).await,
            } {
                warn!("{}", err);
            }
        };
        let handle = tokio::spawn(download);
        Self { r#type: PhantomData, handle, receiver, from, upto }
    }

    fn done_reading(&self, last_read: Option<BlockNumber>) -> bool {
        if let Some(block_number) = last_read {
            if block_number.next() == self.upto {
                debug!("[{:?}]: Done reading task [{}, {}).", D::r#type(), self.from, self.upto);
                return true;
            }
        }
        false
    }
}

async fn download_blocks<T: CentralSourceTrait + Sync + Send>(
    source: Arc<T>,
    sender: mpsc::Sender<SyncData>,
    from: BlockNumber,
    upto: BlockNumber,
) -> Result<(), DownloadsManagerError> {
    debug!("Downloading blocks [{}, {}).", from, upto);
    let block_stream = source.stream_new_blocks(from, upto).fuse();
    pin_mut!(block_stream);

    while let Some(maybe_block) = block_stream.next().await {
        let (block_number, block) = maybe_block?;
        sender.send(SyncData::Block(BlockSyncData { block_number, block })).await.map_err(|e| {
            DownloadsManagerError::Channel {
                msg: format!(
                    "Problem with sending block {block_number} on the channel of the task \
                     [{from}, {upto}): {e}."
                ),
            }
        })?;
        trace!("Downloaded block {block_number}.");
    }

    Ok(())
}

async fn download_state_diffs<T: CentralSourceTrait + Sync + Send>(
    source: Arc<T>,
    sender: mpsc::Sender<SyncData>,
    from: BlockNumber,
    upto: BlockNumber,
) -> Result<(), DownloadsManagerError> {
    debug!("Downloading state diffs [{}, {}).", from, upto);
    let state_diff_stream = source.stream_state_updates(from, upto).fuse();
    pin_mut!(state_diff_stream);

    while let Some(maybe_state_diff) = state_diff_stream.next().await {
        let (block_number, block_hash, state_diff, deployed_contract_class_definitions) =
            maybe_state_diff?;
        sender
            .send(SyncData::StateDiff(StateDiffSyncData {
                block_number,
                block_hash,
                state_diff,
                deployed_contract_class_definitions,
            }))
            .await
            .map_err(|e| DownloadsManagerError::Channel {
                msg: format!(
                    "Problem with sending state diff of block {block_number} on the channel of \
                     the task [{from}, {upto}): {e}."
                ),
            })?;
        trace!("Downloaded state diff of block {block_number}.");
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum DownloadsManagerError {
    #[error(transparent)]
    CentralSource(#[from] CentralError),
    #[error("Channel error - {msg}")]
    Channel { msg: String },
    #[error("Data conversion error - {msg}")]
    DataConversion { msg: String },
    #[error("No task.")]
    NoTask,
    #[error("Task failed.")]
    TaskFailed,
    #[error("Received bad range [{from},{upto}). Current range [{curr_from}, {curr_upto})")]
    BadRange {
        from: BlockNumber,
        upto: BlockNumber,
        curr_from: BlockNumber,
        curr_upto: BlockNumber,
    },
}

#[derive(Debug, Clone)]
pub struct BlockSyncData {
    pub block_number: BlockNumber,
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct StateDiffSyncData {
    pub block_number: BlockNumber,
    pub block_hash: BlockHash,
    pub state_diff: StateDiff,
    // TODO(anatg): Remove once there are no more deployed contracts with undeclared classes.
    // Class definitions of deployed contracts with classes that were not declared in this
    // state diff.
    pub deployed_contract_class_definitions: IndexMap<ClassHash, ContractClass>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SyncData {
    Block(BlockSyncData),
    StateDiff(StateDiffSyncData),
}

pub trait SyncDataTrait: Sized + Sync + Send + Debug {
    fn r#type() -> SyncDataType;
    fn block_number(&self) -> BlockNumber;
    fn try_from(data: SyncData) -> Result<Self, DownloadsManagerError>;
}

impl SyncDataTrait for BlockSyncData {
    fn r#type() -> SyncDataType {
        SyncDataType::Block
    }

    fn block_number(&self) -> BlockNumber {
        self.block_number
    }

    fn try_from(data: SyncData) -> Result<Self, DownloadsManagerError> {
        if let SyncData::Block(block_sync_data) = data {
            return Ok(block_sync_data);
        }
        Err(DownloadsManagerError::DataConversion {
            msg: String::from("Expected block sync data type."),
        })
    }
}

impl SyncDataTrait for StateDiffSyncData {
    fn r#type() -> SyncDataType {
        SyncDataType::StateDiff
    }

    fn block_number(&self) -> BlockNumber {
        self.block_number
    }

    fn try_from(data: SyncData) -> Result<Self, DownloadsManagerError> {
        if let SyncData::StateDiff(state_diff_sync_data) = data {
            return Ok(state_diff_sync_data);
        }
        Err(DownloadsManagerError::DataConversion {
            msg: String::from("Expected state diff sync data type."),
        })
    }
}

#[derive(Debug)]
pub enum SyncDataType {
    Block,
    StateDiff,
}
