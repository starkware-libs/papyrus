use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use deadqueue::unlimited::Queue;
use futures::channel::mpsc::{unbounded, Sender, UnboundedSender};
use futures::future::poll_fn;
use futures::stream::{FuturesUnordered, Stream};
use futures::{pin_mut, Future, FutureExt, SinkExt, StreamExt};
use libp2p::PeerId;
use starknet_api::block::{BlockHeader, BlockNumber, BlockSignature};
use tokio::select;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use super::swarm_trait::{Event, SwarmTrait};
use super::GenericNetworkManager;
use crate::block_headers::behaviour::{PeerNotConnected, SessionIdNotFoundError};
use crate::block_headers::Event as BehaviourEvent;
use crate::db_executor::{poll_query_execution_set, DBExecutor, DBExecutorError, Data, QueryId};
use crate::streamed_data::{InboundSessionId, OutboundSessionId};
use crate::{
    BlockHashOrNumber,
    DataType,
    Direction,
    InternalQuery,
    PeerAddressConfig,
    Query,
    SignedBlockHeader,
};

#[derive(Default)]
struct MockSwarm {
    pub pending_events: Queue<Event>,
    pub sent_queries: Vec<(InternalQuery, PeerId)>,
    inbound_session_id_to_data_sender: HashMap<InboundSessionId, UnboundedSender<Data>>,
    next_outbound_session_id: usize,
}

impl Stream for MockSwarm {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let fut = self.pending_events.pop().map(Some);
        pin_mut!(fut);
        fut.poll_unpin(cx)
    }
}

impl MockSwarm {
    pub fn get_data_sent_to_inbound_session(
        &mut self,
        inbound_session_id: InboundSessionId,
    ) -> impl Future<Output = Vec<Data>> {
        let (data_sender, data_receiver) = unbounded();
        if self.inbound_session_id_to_data_sender.insert(inbound_session_id, data_sender).is_some()
        {
            panic!("Called get_data_sent_to_inbound_session on {inbound_session_id:?} twice");
        }
        data_receiver.collect()
    }

    fn create_received_data_events_for_query(
        &self,
        query: InternalQuery,
        outbound_session_id: OutboundSessionId,
    ) {
        let BlockHashOrNumber::Number(BlockNumber(start_block_number)) = query.start_block else {
            unimplemented!("test does not support start block as block hash")
        };
        let block_max_number = start_block_number + (query.step * query.limit);
        for block_number in (start_block_number..block_max_number)
            .step_by(query.step.try_into().expect("step too large to convert to usize"))
        {
            let signed_header = SignedBlockHeader {
                block_header: BlockHeader {
                    block_number: BlockNumber(block_number),
                    ..Default::default()
                },
                signatures: vec![],
            };
            self.pending_events.push(Event::Behaviour(BehaviourEvent::ReceivedData {
                signed_header,
                outbound_session_id,
            }));
        }
    }
}

impl SwarmTrait for MockSwarm {
    fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let data_sender = self
            .inbound_session_id_to_data_sender
            .get(&inbound_session_id)
            .expect("Called send_data without calling get_data_sent_to_inbound_session first");
        let is_fin = matches!(data, Data::Fin);
        data_sender.unbounded_send(data).unwrap();
        if is_fin {
            data_sender.close_channel();
        }
        Ok(())
    }

    fn send_query(
        &mut self,
        query: InternalQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.sent_queries.push((query, peer_id));
        let outbound_session_id = OutboundSessionId { value: self.next_outbound_session_id };
        self.create_received_data_events_for_query(query, outbound_session_id);
        self.next_outbound_session_id += 1;
        Ok(outbound_session_id)
    }

    fn dial(&mut self, _peer: PeerAddressConfig) -> Result<(), libp2p::swarm::DialError> {
        Ok(())
    }
}

#[derive(Default)]
struct MockDBExecutor {
    next_query_id: usize,
    pub query_to_headers: HashMap<InternalQuery, Vec<BlockHeader>>,
    query_execution_set: FuturesUnordered<JoinHandle<Result<QueryId, DBExecutorError>>>,
}

impl Stream for MockDBExecutor {
    type Item = Result<QueryId, DBExecutorError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        poll_query_execution_set(&mut Pin::into_inner(self).query_execution_set, cx)
    }
}

impl DBExecutor for MockDBExecutor {
    // TODO(shahak): Consider fixing code duplication with BlockHeaderDBExecutor.
    fn register_query(&mut self, query: InternalQuery, mut sender: Sender<Data>) -> QueryId {
        let query_id = QueryId(self.next_query_id);
        self.next_query_id += 1;
        let headers = self.query_to_headers.get(&query).unwrap().clone();
        self.query_execution_set.push(tokio::task::spawn(async move {
            {
                for header in headers.iter().cloned() {
                    // Using poll_fn because Sender::poll_ready is not a future
                    if let Ok(()) = poll_fn(|cx| sender.poll_ready(cx)).await {
                        if let Err(e) = sender.start_send(Data::BlockHeaderAndSignature {
                            header,
                            signature: BlockSignature::default(),
                        }) {
                            return Err(DBExecutorError::SendError { query_id, send_error: e });
                        };
                    }
                }
                if let Ok(()) = poll_fn(|cx| sender.poll_ready(cx)).await {
                    if let Err(e) = sender.start_send(Data::Fin) {
                        return Err(DBExecutorError::SendError { query_id, send_error: e });
                    };
                }
                Ok(query_id)
            }
        }));
        query_id
    }
}

const HEADER_BUFFER_SIZE: usize = 100;

#[tokio::test]
async fn register_subscriber_and_use_channels() {
    // create mocked network manager
    let mut network_manager = GenericNetworkManager::generic_new(
        MockSwarm::default(),
        MockDBExecutor::default(),
        HEADER_BUFFER_SIZE,
        Some(PeerAddressConfig { peer_id: PeerId::random(), ..Default::default() }),
    );
    // define query
    let query_limit = 5;
    let start_block_number = 0;
    let query = Query {
        start_block: BlockNumber(start_block_number),
        direction: Direction::Forward,
        limit: query_limit,
        step: 1,
        data_type: DataType::SignedBlockHeader,
    };

    // register subscriber and send query
    let (mut query_sender, response_receivers) = network_manager.register_subscriber();
    query_sender.send(query).await.unwrap();

    let signed_header_receiver_collector = response_receivers
        .signed_headers_receiver
        .enumerate()
        .take(query_limit)
        .map(|(i, signed_block_header)| {
            assert_eq!(signed_block_header.block_header.block_number.0, i as u64);
            signed_block_header
        })
        .collect::<Vec<_>>();

    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        _ = signed_header_receiver_collector => {}
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
}

#[tokio::test]
async fn process_incoming_query() {
    // Create data for test.
    let query = InternalQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(0)),
        direction: Direction::Forward,
        limit: 5,
        step: 1,
    };
    let headers = (0..5)
        .map(|i| BlockHeader { block_number: BlockNumber(i), ..Default::default() })
        .collect::<Vec<_>>();

    // Setup mock DB executor and tell it to reply to the query with the given headers.
    let mut mock_db_executor = MockDBExecutor::default();
    mock_db_executor.query_to_headers.insert(query, headers.clone());

    // Setup mock swarm and tell it to return an event of new inbound query.
    let mut mock_swarm = MockSwarm::default();
    let inbound_session_id = InboundSessionId { value: 0 };
    mock_swarm
        .pending_events
        .push(Event::Behaviour(BehaviourEvent::NewInboundQuery { query, inbound_session_id }));

    // Create a future that will return when Fin is sent with the data sent on the swarm.
    let get_data_fut = mock_swarm.get_data_sent_to_inbound_session(inbound_session_id);

    let network_manager =
        GenericNetworkManager::generic_new(mock_swarm, mock_db_executor, HEADER_BUFFER_SIZE, None);

    select! {
        inbound_session_data = get_data_fut => {
            let mut expected_data = headers
                .into_iter()
                .map(|header| Data::BlockHeaderAndSignature {
                    header, signature: BlockSignature::default()
                })
                .collect::<Vec<_>>();
            expected_data.push(Data::Fin);
            assert_eq!(inbound_session_data, expected_data);
        }
        _ = network_manager.run() => {
            panic!("GenericNetworkManager::run finished before the session finished");
        }
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
}
