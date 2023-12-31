use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures::Stream;
use starknet_api::block::{BlockHeader, BlockSignature};

use crate::messages::protobuf;
use crate::BlockQuery;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct QueryId(pub usize);

pub enum Data {
    BlockHeaderAndSignature { header: BlockHeader, signature: BlockSignature },
    Fin { block_number: u64 },
}

pub(crate) trait DBExecutor: Stream<Item = (QueryId, Data)> + Unpin {
    fn register_query(&mut self, query: BlockQuery) -> QueryId;
}

struct DummyDBExecutor {
    _data: Vec<protobuf::BlockHeadersResponse>,
    query_id_to_query_and_status: HashMap<QueryId, (BlockQuery, u64)>,
    query_conter: usize,
}

impl DummyDBExecutor {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            _data: DummyDBExecutor::generate_data(),
            query_conter: 0,
            query_id_to_query_and_status: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn generate_data() -> Vec<protobuf::BlockHeadersResponse> {
        let mut data = Vec::with_capacity(100);
        for i in 1..101 {
            data.push(protobuf::BlockHeadersResponse {
                part: vec![protobuf::BlockHeadersResponsePart {
                    header_message: Some(
                        protobuf::block_headers_response_part::HeaderMessage::Header(
                            protobuf::BlockHeader { number: i, ..Default::default() },
                        ),
                    ),
                }],
            })
        }
        data
    }
}

impl Stream for DummyDBExecutor {
    type Item = (QueryId, Data);

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        if let Some((query_id, (query, status))) =
            unpinned_self.query_id_to_query_and_status.iter_mut().next()
        {
            let data = if *status < query.limit {
                *status += 1;
                Data::BlockHeaderAndSignature {
                    header: BlockHeader::default(),
                    signature: BlockSignature::default(),
                }
            } else {
                Data::Fin { block_number: 0 }
            };
            Poll::Ready(Some((*query_id, data)))
        } else {
            Poll::Pending
        }
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
