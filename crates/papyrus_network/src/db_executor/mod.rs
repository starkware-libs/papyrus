use std::cmp::Ordering;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures::Stream;
use starknet_api::block::{BlockHeader, BlockSignature};

use crate::BlockQuery;

pub mod dummy_executor;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct QueryId(pub usize);

pub enum Data {
    BlockHeaderAndSignature { header: BlockHeader, signature: BlockSignature },
    Fin,
}

pub trait DBExecutor: Stream<Item = (QueryId, Data)> + Unpin {
    // TODO: add writer functionality
    fn register_query(&mut self, query: BlockQuery) -> QueryId;

    // Get an active query and the number of blocks read so far.
    // Specific implementations may decide on the strategy.
    fn get_active_query(&mut self) -> Option<(QueryId, &mut BlockQuery, &mut u64)>;

    // Default poll implementation counts query results and returns Fin when done reading.
    // To be used in the poll_next function.
    fn poll_func(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        if let Some((query_id, query, read_blocks_counter)) = unpinned_self.get_active_query() {
            let res = match (*read_blocks_counter).cmp(&query.limit) {
                Ordering::Less => {
                    *read_blocks_counter += 1;
                    Some((
                        query_id,
                        // TODO: get data from actual source.
                        Data::BlockHeaderAndSignature {
                            header: BlockHeader::default(),
                            signature: BlockSignature::default(),
                        },
                    ))
                }
                Ordering::Equal => {
                    *read_blocks_counter += 1;
                    Some((query_id, Data::Fin))
                }
                Ordering::Greater => None,
            };
            Poll::Ready(res)
        } else {
            Poll::Pending
        }
    }
}

// TODO: currently this executor returns only block headers and signatures.
struct BlockHeaderDBExecutor {
    query_id_to_query_and_read_blocks_counter: HashMap<QueryId, (BlockQuery, u64)>,
    query_conter: usize,
}

impl BlockHeaderDBExecutor {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self { query_conter: 0, query_id_to_query_and_read_blocks_counter: HashMap::new() }
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
