use std::pin::Pin;
use std::task::Poll;

use derive_more::Display;
use futures::channel::mpsc::Sender;
use futures::future::poll_fn;
use futures::stream::FuturesUnordered;
use futures::{Stream, StreamExt};
#[cfg(test)]
use mockall::automock;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{db, StorageReader, StorageTxn};
use starknet_api::block::{BlockHeader, BlockNumber, BlockSignature};
use starknet_api::state::ThinStateDiff;
use tokio::task::JoinHandle;

use crate::{BlockHashOrNumber, DataType, InternalQuery};

#[cfg(test)]
mod test;

mod utils;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Display)]
pub struct QueryId(pub usize);

#[cfg_attr(test, derive(Debug, Clone, PartialEq, Eq, Default))]
pub enum Data {
    // TODO(shahak): Consider uniting with SignedBlockHeader.
    BlockHeaderAndSignature {
        header: BlockHeader,
        signatures: Vec<BlockSignature>,
    },
    StateDiff {
        state_diff: ThinStateDiff,
    },
    #[cfg_attr(test, default)]
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
    BlockNumberOutOfRange { query: InternalQuery, counter: u64, query_id: QueryId },
    // TODO: add data type to the error message.
    #[error("Block not found. Block: {block_hash_or_number:?}, query_id: {query_id}")]
    BlockNotFound { block_hash_or_number: BlockHashOrNumber, query_id: QueryId },
    // This error should be non recoverable.
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    // TODO: remove this error, use BlockNotFound instead.
    // This error should be non recoverable.
    #[error(
        "Block {block_number:?} is in the storage but its signature isn't. query_id: {query_id}"
    )]
    SignatureNotFound { block_number: BlockNumber, query_id: QueryId },
    #[error("Send error. Query id: {query_id}, error: {send_error:?}")]
    SendError {
        query_id: QueryId,
        #[source]
        send_error: futures::channel::mpsc::SendError,
    },
}

#[allow(dead_code)]
impl DBExecutorError {
    pub fn query_id(&self) -> Option<QueryId> {
        match self {
            Self::DBInternalError { query_id, .. }
            | Self::BlockNumberOutOfRange { query_id, .. }
            | Self::BlockNotFound { query_id, .. }
            | Self::SignatureNotFound { query_id, .. }
            | Self::SendError { query_id, .. } => Some(*query_id),
            Self::JoinError(_) => None,
        }
    }

    pub fn should_log_in_error_level(&self) -> bool {
        match self {
            Self::JoinError(_) | Self::SignatureNotFound { .. } | Self::SendError { .. }
            // TODO(shahak): Consider returning false for some of the StorageError variants.
            | Self::DBInternalError { .. } => true,
            Self::BlockNumberOutOfRange { .. } | Self::BlockNotFound { .. } => false,
        }
    }
}

/// Db executor is a stream of queries. Each result is marks the end of a query fulfillment.
/// A query can either succeed (and return Ok(QueryId)) or fail (and return Err(DBExecutorError)).
/// The stream is never exhausted, and it is the responsibility of the user to poll it.
pub trait DBExecutor: Stream<Item = Result<QueryId, DBExecutorError>> + Unpin {
    // TODO: add writer functionality
    fn register_query(
        &mut self,
        query: InternalQuery,
        data_type: impl FetchBlockDataFromDb + Send + 'static,
        sender: Sender<Data>,
    ) -> QueryId;
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
    fn register_query(
        &mut self,
        query: InternalQuery,
        data_type: impl FetchBlockDataFromDb + Send + 'static,
        mut sender: Sender<Data>,
    ) -> QueryId {
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
                    let block_number = BlockNumber(utils::calculate_block_number(
                        query,
                        start_block_number,
                        block_counter,
                        query_id,
                    )?);
                    let data = data_type.fetch_block_data_from_db(block_number, query_id, &txn)?;
                    // Using poll_fn because Sender::poll_ready is not a future
                    match poll_fn(|cx| sender.poll_ready(cx)).await {
                        Ok(()) => {
                            if let Err(e) = sender.start_send(data) {
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
        poll_query_execution_set(&mut Pin::into_inner(self).query_execution_set, cx)
    }
}

pub(crate) fn poll_query_execution_set(
    query_execution_set: &mut FuturesUnordered<JoinHandle<Result<QueryId, DBExecutorError>>>,
    cx: &mut std::task::Context<'_>,
) -> Poll<Option<Result<QueryId, DBExecutorError>>> {
    match query_execution_set.poll_next_unpin(cx) {
        Poll::Ready(Some(join_result)) => {
            let res = join_result?;
            Poll::Ready(Some(res))
        }
        Poll::Ready(None) => {
            *query_execution_set = FuturesUnordered::new();
            Poll::Pending
        }
        Poll::Pending => Poll::Pending,
    }
}

#[cfg_attr(test, automock)]
// we need to tell clippy to ignore the "needless" lifetime warning because it's not true.
// we do need the lifetime for the automock, following clippy's suggestion will break the code.
#[allow(clippy::needless_lifetimes)]
pub trait FetchBlockDataFromDb {
    fn fetch_block_data_from_db<'a>(
        &self,
        block_number: BlockNumber,
        query_id: QueryId,
        txn: &StorageTxn<'a, db::RO>,
    ) -> Result<Data, DBExecutorError>;
}

impl FetchBlockDataFromDb for DataType {
    fn fetch_block_data_from_db(
        &self,
        block_number: BlockNumber,
        query_id: QueryId,
        txn: &StorageTxn<'_, db::RO>,
    ) -> Result<Data, DBExecutorError> {
        match self {
            DataType::SignedBlockHeader => {
                let header = txn
                    .get_block_header(block_number)
                    .map_err(|err| DBExecutorError::DBInternalError {
                        query_id,
                        storage_error: err,
                    })?
                    .ok_or(DBExecutorError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                        query_id,
                    })?;
                let signature = txn
                    .get_block_signature(block_number)
                    .map_err(|err| DBExecutorError::DBInternalError {
                        query_id,
                        storage_error: err,
                    })?
                    .ok_or(DBExecutorError::SignatureNotFound { block_number, query_id })?;
                Ok(Data::BlockHeaderAndSignature { header, signatures: vec![signature] })
            }
            DataType::StateDiff => {
                let state_diff = txn
                    .get_state_diff(block_number)
                    .map_err(|err| DBExecutorError::DBInternalError {
                        query_id,
                        storage_error: err,
                    })?
                    .ok_or(DBExecutorError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                        query_id,
                    })?;
                Ok(Data::StateDiff { state_diff })
            }
        }
    }
}
