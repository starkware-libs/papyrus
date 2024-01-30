use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures::channel::mpsc::Sender;
use futures::Stream;

use super::{DBExecutor, DBExecutorError, Data, QueryId};
use crate::messages::protobuf;
use crate::BlockQuery;

pub struct DummyDBExecutor {
    _data: Vec<protobuf::BlockHeadersResponse>,
    query_id_to_query_and_read_blocks_counter: HashMap<QueryId, (BlockQuery, Sender<Data>)>,
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

    fn get_active_query(&mut self) -> Option<(QueryId, &mut BlockQuery, &mut Sender<Data>)> {
        self.query_id_to_query_and_read_blocks_counter
            .iter_mut()
            .next()
            .map(|(query_id, (query, sender))| (*query_id, query, sender))
    }
}

impl DBExecutor for DummyDBExecutor {
    fn register_query(&mut self, query: BlockQuery, sender: Sender<Data>) -> QueryId {
        let query_id = QueryId(self.query_conter);
        self.query_conter += 1;
        self.query_id_to_query_and_read_blocks_counter.insert(query_id, (query, sender));
        query_id
    }
}

impl Stream for DummyDBExecutor {
    type Item = Result<QueryId, DBExecutorError>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        let Some((query_id, query, sender)) = unpinned_self.get_active_query() else {
            return Poll::Pending;
        };
        // TODO: add a way to configure an expected failure for a query.
        for _blocks_counter in 0..=query.limit {
            // TODO: use generated data instead of creating default data in each iteration.
            let data = Data::BlockHeaderAndSignature {
                header: Default::default(),
                signature: Some(Default::default()),
            };
            if let Err(e) = sender.try_send(data) {
                panic!("failed to send data to sender. error: {:?}", e);
            }
        }
        if let Err(e) = sender.try_send(Data::Fin) {
            panic!("failed to send fin to sender. error: {:?}", e);
        }
        unpinned_self.query_id_to_query_and_read_blocks_counter.remove(&query_id);
        Poll::Ready(Some(Ok(query_id)))
    }
}
