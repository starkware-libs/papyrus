use std::pin::Pin;
use std::task::Poll;

use derive_more::Display;
use futures::channel::mpsc::Sender;
use futures::future::poll_fn;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHeader, BlockNumber, BlockSignature};
use tokio::task::JoinHandle;

use crate::{BlockHashOrNumber, BlockQuery};

pub mod dummy_executor;
#[cfg(test)]
mod test;
mod utils;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Display)]
pub struct QueryId(pub usize);

#[cfg_attr(test, derive(Debug))]
pub enum Data {
    BlockHeaderAndSignature { header: BlockHeader, signature: Option<BlockSignature> },
    Fin,
}

#[derive(thiserror::Error, Debug)]
pub enum DBExecutorError {
    #[error("Storage error. Query id: {query_id}, error: {storage_error:?}")]
    DBInternalError {
        query_id: QueryId,
        #[source]
        storage_error: papyrus_storage::StorageError,
    },
    #[error(
        "Block number is out of range. Query: {query:?}, counter: {counter}, query_id: {query_id}"
    )]
    BlockNumberOutOfRange { query: BlockQuery, counter: u64, query_id: QueryId },
    #[error("Block not found. Block: {block_hash_or_number:?}, query_id: {query_id}")]
    BlockNotFound { block_hash_or_number: BlockHashOrNumber, query_id: QueryId },
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    #[error("Send error. Query id: {query_id}, error: {send_error:?}")]
    SendError {
        query_id: QueryId,
        #[source]
        send_error: futures::channel::mpsc::SendError,
    },
}

/// Db executor is a stream of queries. Each result is marks the end of a query fulfillment.
/// A query can either succeed (and return Ok(QueryId)) or fail (and return Err(DBExecutorError)).
/// The stream is never exhausted, and it is the responsibility of the user to poll it.
pub trait DBExecutor: Stream<Item = Result<QueryId, DBExecutorError>> + Unpin {
    // TODO: add writer functionality
    fn register_query(&mut self, query: BlockQuery, sender: Sender<Data>) -> QueryId;
}

// TODO: currently this executor returns only block headers and signatures.
pub struct BlockHeaderDBExecutor {
    next_query_id: usize,
    storage_reader: StorageReader,
    query_execution_set: FuturesUnordered<JoinHandle<Result<QueryId, DBExecutorError>>>,
}

impl BlockHeaderDBExecutor {
    #[allow(dead_code)]
    pub fn new(storage_reader: StorageReader) -> Self {
        Self { next_query_id: 0, storage_reader, query_execution_set: FuturesUnordered::new() }
    }
}

impl DBExecutor for BlockHeaderDBExecutor {
    fn register_query(&mut self, query: BlockQuery, mut sender: Sender<Data>) -> QueryId {
        // TODO: consider create a sized vector and increase its size when needed.
        let query_id = QueryId(self.next_query_id);
        self.next_query_id += 1;
        let storage_reader_clone = self.storage_reader.clone();
        self.query_execution_set.push(tokio::task::spawn(async move {
            {
                let txn = storage_reader_clone.begin_ro_txn().map_err(|err| {
                    DBExecutorError::DBInternalError { query_id, storage_error: err }
                })?;
                let start_block_number = match query.start_block {
                    BlockHashOrNumber::Number(BlockNumber(num)) => num,
                    BlockHashOrNumber::Hash(block_hash) => {
                        txn.get_block_number_by_hash(&block_hash)
                            .map_err(|err| DBExecutorError::DBInternalError {
                                query_id,
                                storage_error: err,
                            })?
                            .ok_or(DBExecutorError::BlockNotFound {
                                block_hash_or_number: BlockHashOrNumber::Hash(block_hash),
                                query_id,
                            })?
                            .0
                    }
                };
                for block_counter in 0..query.limit {
                    let block_number = utils::calculate_block_number(
                        query,
                        start_block_number,
                        block_counter,
                        query_id,
                    )?;
                    let header = txn
                        .get_block_header(BlockNumber(block_number))
                        .map_err(|err| DBExecutorError::DBInternalError {
                            query_id,
                            storage_error: err,
                        })?
                        .ok_or(DBExecutorError::BlockNotFound {
                            block_hash_or_number: BlockHashOrNumber::Number(BlockNumber(
                                block_number,
                            )),
                            query_id,
                        })?;
                    // Using poll_fn because Sender::poll_ready is not a future
                    match poll_fn(|cx| sender.poll_ready(cx)).await {
                        Ok(()) => {
                            if let Err(e) = sender.start_send(Data::BlockHeaderAndSignature {
                                header,
                                signature: None,
                            }) {
                                // TODO: consider implement retry mechanism.
                                return Err(DBExecutorError::SendError { query_id, send_error: e });
                            };
                        }
                        Err(e) => {
                            return Err(DBExecutorError::SendError { query_id, send_error: e });
                        }
                    }
                }
                Ok(query_id)
            }
        }));
        query_id
    }
}

impl Stream for BlockHeaderDBExecutor {
    type Item = Result<QueryId, DBExecutorError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        match unpinned_self.query_execution_set.poll_next_unpin(cx) {
            Poll::Ready(Some(join_result)) => {
                let res = join_result?;
                Poll::Ready(Some(res))
            }
            Poll::Ready(None) => {
                unpinned_self.query_execution_set = FuturesUnordered::new();
                Poll::Pending
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
