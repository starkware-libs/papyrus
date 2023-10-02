use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures::Stream;
use starknet_api::block::{Block, BlockHeader};

use crate::messages::protobuf;
use crate::BlockQuery;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct QueryId(pub usize);

pub enum Data {
    BlockHeader(BlockHeader),
    Fin { block_number: u64 },
}

pub(crate) trait DBExecutor: Stream<Item = (QueryId, Data)> + Unpin {
    fn register_query(&mut self, query: BlockQuery) -> QueryId;
}

struct DummyDBExecutor {
    data: Vec<protobuf::BlockHeadersResponse>,
    query_id_to_query_and_status: HashMap<QueryId, (BlockQuery, u64)>,
    query_conter: usize,
}

impl DummyDBExecutor {
    pub fn new() -> Self {
        Self {
            data: DummyDBExecutor::generate_data(),
            query_conter: 0,
            query_id_to_query_and_status: HashMap::new(),
        }
    }

    fn generate_data() -> Vec<protobuf::BlockHeadersResponse> {
        let mut data = Vec::with_capacity(100);
        for i in 1..101 {
            data.push(protobuf::BlockHeadersResponse {
                block_number: i,
                header_message: Some(protobuf::block_headers_response::HeaderMessage::Header(
                    protobuf::BlockHeader { number: i, ..Default::default() },
                )),
            })
        }
        data
    }

    // fn get_next_data(&self) -> Some(BlockHeader) {
    //     let endless_data = self.data.iter().cycle();
    //     yield endless_data.next()
    // }
}

impl Stream for DummyDBExecutor {
    type Item = (QueryId, Data);

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        for (query_id, (query, status)) in unpinned_self.query_id_to_query_and_status.iter_mut() {
            let data;
            if *status < query.limit {
                *status += 1;
                data = Data::BlockHeader(BlockHeader::default());
            } else {
                data = Data::Fin { block_number: 0 };
            };
            return Poll::Ready(Some((*query_id, data)));
        }
        Poll::Pending
    }
}

impl DBExecutor for DummyDBExecutor {
    fn register_query(&mut self, query: BlockQuery) -> QueryId {
        let query_id = QueryId(self.query_conter);
        self.query_conter += 1;
        self.query_id_to_query_and_status.insert(query_id, (query, 0));
        query_id
    }
}
