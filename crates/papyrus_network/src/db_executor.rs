use std::cmp::Ordering;
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
    Fin,
}

pub(crate) trait DBExecutor: Stream<Item = (QueryId, Data)> + Unpin {
    // TODO: add writer functionality
    fn register_query(&mut self, query: BlockQuery) -> QueryId;
}

pub struct DummyDBExecutor {
    _data: Vec<protobuf::BlockHeadersResponse>,
    query_id_to_query_and_read_blocks_counter: HashMap<QueryId, (BlockQuery, u64)>,
    query_conter: usize,
}

impl DummyDBExecutor {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            _data: DummyDBExecutor::generate_data(),
            query_conter: 0,
            query_id_to_query_and_read_blocks_counter: HashMap::new(),
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
        if let Some((query_id, (query, read_blocks_counter))) =
            unpinned_self.query_id_to_query_and_read_blocks_counter.iter_mut().next()
        {
            let res = match (*read_blocks_counter).cmp(&query.limit) {
                Ordering::Less => {
                    *read_blocks_counter += 1;
                    Some((
                        *query_id,
                        Data::BlockHeaderAndSignature {
                            header: BlockHeader::default(),
                            signature: BlockSignature::default(),
                        },
                    ))
                }
                Ordering::Equal => {
                    *read_blocks_counter += 1;
                    Some((*query_id, Data::Fin))
                }
                Ordering::Greater => None,
            };
            Poll::Ready(res)
        } else {
            Poll::Pending
        }
    }
}

impl DBExecutor for DummyDBExecutor {
    fn register_query(&mut self, query: BlockQuery) -> QueryId {
        let query_id = QueryId(self.query_conter);
        self.query_conter += 1;
        self.query_id_to_query_and_read_blocks_counter.insert(query_id, (query, 0));
        query_id
    }
}
