use std::cmp::Ordering;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures::Stream;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHeader, BlockNumber, BlockSignature};

use crate::{BlockQuery, Direction};

pub mod dummy_executor;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct QueryId(pub usize);

pub enum Data {
    BlockHeaderAndSignature { header: BlockHeader, signature: Option<BlockSignature> },
    Fin,
}

#[derive(thiserror::Error, Debug)]
pub enum DBExecutorError {
    #[error(transparent)]
    DBInternalError(#[from] papyrus_storage::StorageError),
    #[error("Block number is out of range. Query: {query:?}, counter: {counter}")]
    BlockNumberOutOfRange { query: BlockQuery, counter: u64 },
    #[error("Block not found. Block number: {0}")]
    BlockNotFound(u64),
}

pub trait DBExecutor: Stream<Item = (QueryId, Data)> + Unpin {
    // TODO: add writer functionality
    fn register_query(&mut self, query: BlockQuery) -> QueryId;

    // Get an active query and the number of blocks read so far.
    // Specific implementations may decide on the strategy.
    fn get_active_query(&mut self) -> Option<(QueryId, &mut BlockQuery, &mut u64)>;

    // Fetch a single data instance from the DB.
    fn fetch_data(
        &mut self,
        query: BlockQuery,
        read_blocks_counter: u64,
    ) -> Result<Data, DBExecutorError>;

    // Default poll implementation counts query results and returns Fin when done reading.
    // To be used in the poll_next function.
    fn poll_func(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        let Some((query_id, query, read_blocks_counter)) = unpinned_self.get_active_query() else {
            return Poll::Pending;
        };
        let res = match (*read_blocks_counter).cmp(&query.limit) {
            Ordering::Less => {
                *read_blocks_counter += 1;
                let query_copy = *query;
                let read_blocks_counter_copy = *read_blocks_counter;
                if let Ok(data) = unpinned_self.fetch_data(query_copy, read_blocks_counter_copy) {
                    Some((query_id, data))
                } else {
                    // TODO: decide what is the right way to handle db errors.
                    None
                }
            }
            Ordering::Equal => {
                *read_blocks_counter += 1;
                Some((query_id, Data::Fin))
            }
            Ordering::Greater => None,
        };
        Poll::Ready(res)
    }
}

// TODO: currently this executor returns only block headers and signatures.
pub struct BlockHeaderDBExecutor {
    query_id_to_query_and_read_blocks_counter: HashMap<QueryId, (BlockQuery, u64)>,
    query_conter: usize,
    storage_reader: StorageReader,
}

impl BlockHeaderDBExecutor {
    #[allow(dead_code)]
    pub fn new(storage_reader: StorageReader) -> Self {
        Self {
            query_conter: 0,
            query_id_to_query_and_read_blocks_counter: HashMap::new(),
            storage_reader,
        }
    }

    fn calc_block_number(
        &self,
        query: BlockQuery,
        read_blocks_counter: u64,
    ) -> Result<u64, DBExecutorError> {
        let direction_factor: i128 = match query.direction {
            Direction::Forward => 1,
            Direction::Backward => -1,
        };
        let blocks_delta: i128 = direction_factor * (query.step * read_blocks_counter) as i128;
        let block_number: i128 = query.start_block.0 as i128 + blocks_delta;
        if block_number <= 0 || block_number > u64::MAX as i128 {
            return Err(DBExecutorError::BlockNumberOutOfRange {
                query,
                counter: read_blocks_counter,
            });
        }
        Ok(block_number as u64)
    }
}

impl DBExecutor for BlockHeaderDBExecutor {
    fn register_query(&mut self, query: BlockQuery) -> QueryId {
        // TODO: when registering a query we should state what type of data we want to receive.
        let query_id = QueryId(self.query_conter);
        self.query_conter += 1;
        self.query_id_to_query_and_read_blocks_counter.insert(query_id, (query, 0));
        query_id
    }

    fn get_active_query(&mut self) -> Option<(QueryId, &mut BlockQuery, &mut u64)> {
        self.query_id_to_query_and_read_blocks_counter
            .iter_mut()
            .next()
            .map(|(query_id, (query, read_blocks_counter))| (*query_id, query, read_blocks_counter))
    }

    fn fetch_data(
        &mut self,
        query: BlockQuery,
        read_blocks_counter: u64,
    ) -> Result<Data, DBExecutorError> {
        let txn = self.storage_reader.begin_ro_txn()?;
        let block_number = self.calc_block_number(query, read_blocks_counter)?;
        let header = txn
            .get_block_header(BlockNumber(block_number))?
            .ok_or_else(|| DBExecutorError::BlockNotFound(block_number))?;
        Ok(Data::BlockHeaderAndSignature { header, signature: None })
    }
}

impl Stream for BlockHeaderDBExecutor {
    type Item = (QueryId, Data);

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.poll_func(cx)
    }
}
