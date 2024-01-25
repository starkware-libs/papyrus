use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures::Stream;

use super::{DBExecutor, Data, QueryId};
use crate::messages::protobuf;
use crate::BlockQuery;

pub struct DummyDBExecutor {
    _data: Vec<protobuf::BlockHeadersResponse>,
    query_id_to_query_and_read_blocks_counter: HashMap<QueryId, (BlockQuery, u64)>,
    query_conter: usize,
}

impl DummyDBExecutor {
    #[allow(dead_code)]
    // TODO: remove allow dead code when possible
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

impl DBExecutor for DummyDBExecutor {
    fn register_query(&mut self, query: BlockQuery) -> QueryId {
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

impl Stream for DummyDBExecutor {
    type Item = (QueryId, Data);

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.poll_func(cx)
    }
}
