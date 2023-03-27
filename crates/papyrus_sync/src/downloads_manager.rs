use std::cmp::min;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use starknet_api::block::BlockNumber;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};

use crate::data::{SyncData, SyncDataError, SyncDataTrait};
use crate::sources::CentralSourceTrait;

// Orchestrates specific network interfaces (e.g. central, p2p, l1) and sends data to the sync.
pub struct DownloadsManager<T, D>
where
    T: CentralSourceTrait + Sync + Send,
    D: SyncDataTrait,
{
    tasks: Vec<Task<T, D>>,
    source: Arc<T>,
    max_active_tasks: u16,
    max_range_per_task: u16,
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
        max_active_tasks: u16,
        max_range_per_task: u16,
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
        for _i in 0..self.max_active_tasks - (active_tasks.len() as u16) {
            let task_upto = BlockNumber(min(from.0 + u64::from(self.max_range_per_task), upto.0));
            debug!("[{:?}]: Creating task [{}, {}).", D::r#type(), from, task_upto);
            let task = Task::new(self.source.clone(), from, task_upto, self.max_range_per_task);
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
    fn new(source: Arc<T>, from: BlockNumber, upto: BlockNumber, max_range_per_task: u16) -> Self {
        let (sender, receiver) = mpsc::channel(max_range_per_task.into());
        let download = async move {
            if let Err(err) = D::download(source, sender, from, upto).await {
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

#[derive(thiserror::Error, Debug)]
pub enum DownloadsManagerError {
    #[error(transparent)]
    Data(#[from] SyncDataError),
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
