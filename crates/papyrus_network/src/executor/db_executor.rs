use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};

use crate::{BlockQuery, BlockResult};

#[allow(dead_code)]
pub(crate) struct DbExecutor<'a> {
    query_receiver: UnboundedReceiver<BlockQuery>,
    query_sender: UnboundedSender<BlockQuery>,
    blocks_data_sender: UnboundedSender<BlockResult>,
    db_ops: FuturesUnordered<BoxFuture<'a, BlockResult>>,
    status: DbExecutorStatus,
    blocks_data_reciever: Option<UnboundedReceiver<BlockResult>>,
}

#[allow(dead_code)]
impl<'a> DbExecutor<'a> {
    pub(crate) fn new() -> Self {
        let (query_sender, query_receiver) = unbounded();
        let (blocks_data_sender, blocks_data_receiver) = unbounded();
        Self {
            query_receiver,
            query_sender,
            blocks_data_sender,
            blocks_data_reciever: Some(blocks_data_receiver),
            db_ops: FuturesUnordered::new(),
            status: DbExecutorStatus::new(),
        }
    }

    pub(crate) fn get_query_sender(&self) -> UnboundedSender<BlockQuery> {
        self.query_sender.clone()
    }

    pub(crate) fn get_blocks_data_receiver(&mut self) -> Option<UnboundedReceiver<BlockResult>> {
        self.blocks_data_reciever.take()
    }

    #[cfg(test)]
    pub(crate) fn get_query_receiver(&mut self) -> &mut UnboundedReceiver<BlockQuery> {
        &mut self.query_receiver
    }

    #[cfg(test)]
    pub(crate) fn get_db_ops(&self) -> &FuturesUnordered<BoxFuture<'a, BlockResult>> {
        &self.db_ops
    }
}

/// The status of the executor.
/// should include statistics of the executer so the load can be managed
/// e.g. number of open queries
#[derive(Clone, Debug)]
pub(crate) struct DbExecutorStatus {
    pub open_queries: usize,
    pub query_receiver_terminated: bool,
    pub data_items_sent: usize,
    pub block_data_channel_terminated: bool,
}

impl DbExecutorStatus {
    fn new() -> Self {
        Self {
            open_queries: 0,
            query_receiver_terminated: false,
            data_items_sent: 0,
            block_data_channel_terminated: false,
        }
    }
}

impl<'a> Stream for DbExecutor<'a> {
    type Item = DbExecutorStatus;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut new_status = self.status.clone();
        // try to receive new queries
        if !self.status.query_receiver_terminated {
            match self.query_receiver.poll_next_unpin(cx) {
                Poll::Ready(Some(_query)) => {
                    // TODO: create a stream to read from the db
                    // if the data is ordered return FuturesOrdered is it is not return
                    // FuturesUnordered note: there is a problem pushing
                    // FuturesUnordered into db_ops and not extending it. need to resolve it.
                    self.db_ops.extend(FuturesUnordered::<BoxFuture<'_, BlockResult>>::new());
                    new_status.open_queries = self.db_ops.len();
                }
                Poll::Ready(None) => {
                    new_status.query_receiver_terminated = true;
                }
                Poll::Pending => {}
            };
        }
        // currently only one cosumer, check if it's terminated
        if self.status.block_data_channel_terminated {
            return Poll::Ready(None);
        }
        // try to receive new data
        match Pin::new(&mut self.db_ops).poll_next(cx) {
            // got new data, send it to the right consumer
            Poll::Ready(Some(block_result)) => {
                // currently we only have one consumer - blocks_data_sender
                // TODO: once we have more consumers check if the consumer is terminated before
                // trying to send
                if self.blocks_data_sender.unbounded_send(block_result).is_err() {
                    new_status.block_data_channel_terminated = true;
                    self.status = new_status.clone();
                    return Poll::Ready(Some(new_status));
                }
                // update the status and send to the caller
                new_status.open_queries = self.db_ops.len();
                new_status.data_items_sent += 1;
                self.status = new_status.clone();
                Poll::Ready(Some(new_status))
            }
            // db_ops is empty
            Poll::Ready(None) => {
                new_status.open_queries = 0;
                self.status = new_status.clone();
                // if the query receiver is terminated and there are no open queries, terminate the
                // stream
                if self.status.query_receiver_terminated {
                    Poll::Ready(None)
                } else {
                    // otherwise, return the new status
                    Poll::Ready(Some(new_status))
                }
            }
            // if we have no new data, return the current status
            Poll::Pending => {
                new_status.open_queries = self.db_ops.len();
                self.status = new_status.clone();
                Poll::Ready(Some(new_status))
            }
        }
    }
}
