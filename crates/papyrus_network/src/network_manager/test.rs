// TODO(shahak): Remove protobuf from these tests.

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use std::vec;

use deadqueue::unlimited::Queue;
use futures::channel::mpsc::{unbounded, Sender, UnboundedSender};
use futures::channel::oneshot;
use futures::future::{poll_fn, FutureExt};
use futures::stream::{FuturesUnordered, Stream};
use futures::{pin_mut, Future, SinkExt, StreamExt};
use libp2p::core::ConnectedPoint;
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
use papyrus_protobuf::protobuf;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    Direction,
    HeaderQuery,
    Query,
    SignedBlockHeader,
};
use prost::Message;
use starknet_api::block::{BlockHeader, BlockNumber};
use tokio::select;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use super::swarm_trait::{Event, SwarmTrait};
use super::{GenericNetworkManager, SqmrSubscriberChannels};
use crate::db_executor::{
    poll_query_execution_set,
    DBExecutorError,
    DBExecutorTrait,
    Data,
    FetchBlockDataFromDb,
    QueryId,
};
use crate::gossipsub_impl::{self, Topic};
use crate::sqmr::behaviour::{PeerNotConnected, SessionIdNotFoundError};
use crate::sqmr::{Bytes, GenericEvent, InboundSessionId, OutboundSessionId};
use crate::{mixed_behaviour, DataType};

const TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Default)]
struct MockSwarm {
    pub pending_events: Queue<Event>,
    pub sent_queries: Vec<(Query, PeerId)>,
    pub subscribed_topics: HashSet<TopicHash>,
    broadcasted_messages_senders: Vec<UnboundedSender<(Bytes, TopicHash)>>,
    reported_peer_senders: Vec<UnboundedSender<PeerId>>,
    inbound_session_id_to_data_sender: HashMap<InboundSessionId, UnboundedSender<Data>>,
    next_outbound_session_id: usize,
    first_polled_event_notifier: Option<oneshot::Sender<()>>,
    inbound_session_closed_notifier: Option<oneshot::Sender<()>>,
}

impl Stream for MockSwarm {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut_self = self.get_mut();
        let mut fut = mut_self.pending_events.pop().map(Some).boxed();
        if let Some(sender) = mut_self.first_polled_event_notifier.take() {
            fut = fut
                .then(|res| async {
                    sender.send(()).unwrap();
                    res
                })
                .boxed();
        };
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

    pub fn stream_messages_we_broadcasted(&mut self) -> impl Stream<Item = (Bytes, TopicHash)> {
        let (sender, receiver) = unbounded();
        self.broadcasted_messages_senders.push(sender);
        receiver
    }

    pub fn get_reported_peers_stream(&mut self) -> impl Stream<Item = PeerId> {
        let (sender, receiver) = unbounded();
        self.reported_peer_senders.push(sender);
        receiver
    }

    fn create_received_data_events_for_query(
        &self,
        query: Query,
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
            let data_bytes =
                protobuf::BlockHeadersResponse::from(Some(signed_header)).encode_to_vec();
            self.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
                mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::ReceivedData {
                    data: data_bytes,
                    outbound_session_id,
                }),
            )));
        }
    }
}

impl SwarmTrait for MockSwarm {
    fn send_length_prefixed_data(
        &mut self,
        data: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let data_sender = self.inbound_session_id_to_data_sender.get(&inbound_session_id).expect(
            "Called send_length_prefixed_data without calling get_data_sent_to_inbound_session \
             first",
        );
        let decoded_data = protobuf::BlockHeadersResponse::decode_length_delimited(&data[..])
            .unwrap()
            .try_into()
            .unwrap();
        // TODO(shahak): Add tests for state diff.
        let (data, is_fin) = match decoded_data {
            Some(signed_block_header) => {
                (Data::BlockHeaderAndSignature(signed_block_header), false)
            }
            None => (Data::Fin(DataType::SignedBlockHeader), true),
        };
        data_sender.unbounded_send(data).unwrap();
        if is_fin {
            data_sender.close_channel();
        }
        Ok(())
    }

    fn send_query(
        &mut self,
        query: Vec<u8>,
        peer_id: PeerId,
        _protocol: crate::Protocol,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        let query: Query = protobuf::BlockHeadersRequest::decode(&query[..])
            .expect("failed to decode protobuf BlockHeadersRequest")
            .try_into()
            .expect("failed to convert BlockHeadersRequest");
        self.sent_queries.push((query.clone(), peer_id));
        let outbound_session_id = OutboundSessionId { value: self.next_outbound_session_id };
        self.create_received_data_events_for_query(query, outbound_session_id);
        self.next_outbound_session_id += 1;
        Ok(outbound_session_id)
    }

    fn dial(&mut self, _peer: Multiaddr) -> Result<(), libp2p::swarm::DialError> {
        Ok(())
    }
    fn num_connected_peers(&self) -> usize {
        0
    }
    fn close_inbound_session(
        &mut self,
        _session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        if let Some(sender) = self.inbound_session_closed_notifier.take() {
            sender.send(()).unwrap();
        }
        Ok(())
    }

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour {
        unimplemented!()
    }

    fn add_external_address(&mut self, _address: Multiaddr) {}

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError> {
        self.subscribed_topics.insert(topic.hash());
        Ok(())
    }

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash) {
        for sender in &self.broadcasted_messages_senders {
            sender.unbounded_send((message.clone(), topic_hash.clone())).unwrap();
        }
    }

    fn report_peer(&mut self, peer_id: PeerId) {
        for sender in &self.reported_peer_senders {
            sender.unbounded_send(peer_id).unwrap();
        }
    }
}

#[derive(Default)]
struct MockDBExecutor {
    next_query_id: usize,
    pub query_to_headers: HashMap<Query, Vec<BlockHeader>>,
    query_execution_set: FuturesUnordered<JoinHandle<Result<QueryId, DBExecutorError>>>,
}

impl Stream for MockDBExecutor {
    type Item = Result<QueryId, DBExecutorError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        poll_query_execution_set(&mut Pin::into_inner(self).query_execution_set, cx)
    }
}

impl DBExecutorTrait for MockDBExecutor {
    // TODO(shahak): Consider fixing code duplication with DBExecutor.
    fn register_query(
        &mut self,
        query: Query,
        _data_type: impl FetchBlockDataFromDb + Send,
        mut sender: Sender<Vec<Data>>,
    ) -> QueryId {
        let query_id = QueryId(self.next_query_id);
        self.next_query_id += 1;
        let headers = self.query_to_headers.get(&query).unwrap().clone();
        self.query_execution_set.push(tokio::task::spawn(async move {
            {
                for header in headers.iter().cloned() {
                    // Using poll_fn because Sender::poll_ready is not a future
                    if let Ok(()) = poll_fn(|cx| sender.poll_ready(cx)).await {
                        if let Err(e) = sender.start_send(vec![Data::BlockHeaderAndSignature(
                            SignedBlockHeader { block_header: header, signatures: vec![] },
                        )]) {
                            return Err(DBExecutorError::SendError { query_id, send_error: e });
                        };
                    }
                }
                Ok(query_id)
            }
        }));
        query_id
    }
}

const BUFFER_SIZE: usize = 100;

#[tokio::test]
async fn register_subscriber_and_use_channels() {
    // mock swarm to send and track connection established event
    let mut mock_swarm = MockSwarm::default();
    let peer_id = PeerId::random();
    mock_swarm.pending_events.push(get_test_connection_established_event(peer_id));
    let (event_notifier, mut event_listner) = oneshot::channel();
    mock_swarm.first_polled_event_notifier = Some(event_notifier);

    // network manager to register subscriber and send query
    let mut network_manager =
        GenericNetworkManager::generic_new(mock_swarm, MockDBExecutor::default(), BUFFER_SIZE);
    // define query
    let query_limit: usize = 5;
    let start_block_number = 0;
    let query = Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
        direction: Direction::Forward,
        limit: query_limit.try_into().unwrap(),
        step: 1,
    };

    // register subscriber and send query
    let SqmrSubscriberChannels { mut query_sender, response_receiver } = network_manager
        .register_sqmr_subscriber::<HeaderQuery, DataOrFin<SignedBlockHeader>>(
            crate::Protocol::SignedBlockHeader,
        );

    let response_receiver_length = Arc::new(Mutex::new(0));
    let cloned_response_receiver_length = Arc::clone(&response_receiver_length);
    let response_receiver_collector = response_receiver
        .enumerate()
        .take(query_limit)
        .map(|(i, (signed_block_header_result, _report_callback))| {
            let signed_block_header = signed_block_header_result.unwrap();
            assert_eq!(
                signed_block_header.clone().0.unwrap().block_header.block_number.0,
                i as u64
            );
            signed_block_header
        })
        .collect::<Vec<_>>();
    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        _ = poll_fn(|cx| event_listner.poll_unpin(cx)).then(|_| async move {
            query_sender.send(HeaderQuery(query)).await.unwrap()})
            .then(|_| async move {
                *cloned_response_receiver_length.lock().await = response_receiver_collector.await.len();
            }) => {},
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
    assert_eq!(*response_receiver_length.lock().await, query_limit);
}

#[tokio::test]
async fn process_incoming_query() {
    // Create data for test.
    const BLOCK_NUM: u64 = 0;
    let query = Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(BLOCK_NUM)),
        direction: Direction::Forward,
        limit: 5,
        step: 1,
    };
    let headers = (0..5)
        // TODO(shahak): Remove state_diff_length from here once we correctly deduce if it should
        // be None or Some.
        .map(|i| BlockHeader { block_number: BlockNumber(i), state_diff_length: Some(0), ..Default::default() })
        .collect::<Vec<_>>();

    // Setup mock DB executor and tell it to reply to the query with the given headers.
    let mut mock_db_executor = MockDBExecutor::default();
    mock_db_executor.query_to_headers.insert(query.clone(), headers.clone());

    // Setup mock swarm and tell it to return an event of new inbound query.
    let mut mock_swarm = MockSwarm::default();
    let inbound_session_id = InboundSessionId { value: 0 };
    let mut query_bytes = vec![];
    protobuf::BlockHeadersRequest {
        iteration: Some(protobuf::Iteration {
            start: Some(protobuf::iteration::Start::BlockNumber(BLOCK_NUM)),
            direction: protobuf::iteration::Direction::Forward as i32,
            limit: query.limit,
            step: query.step,
        }),
    }
    .encode(&mut query_bytes)
    .unwrap();
    mock_swarm.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
        mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::NewInboundSession {
            query: query_bytes,
            inbound_session_id,
            peer_id: PeerId::random(),
            protocol_name: crate::Protocol::SignedBlockHeader.into(),
        }),
    )));

    // Create a future that will return when Fin is sent with the data sent on the swarm.
    let get_data_fut = mock_swarm.get_data_sent_to_inbound_session(inbound_session_id);

    let network_manager =
        GenericNetworkManager::generic_new(mock_swarm, mock_db_executor, BUFFER_SIZE);

    select! {
        inbound_session_data = get_data_fut => {
            let mut expected_data = headers
                .into_iter()
                .map(|header| {
                    Data::BlockHeaderAndSignature(SignedBlockHeader {
                        block_header: header, signatures: vec![]
                    })
                })
                .collect::<Vec<_>>();
            expected_data.push(Data::Fin(DataType::SignedBlockHeader));
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

#[tokio::test]
async fn close_inbound_session() {
    // define query
    let query_limit = 5;
    let start_block_number = 0;
    let query = Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
        direction: Direction::Forward,
        limit: query_limit,
        step: 1,
    };

    // get query bytes
    let mut query_bytes = vec![];
    protobuf::BlockHeadersRequest {
        iteration: Some(protobuf::Iteration {
            start: Some(protobuf::iteration::Start::BlockNumber(start_block_number)),
            direction: protobuf::iteration::Direction::Forward as i32,
            limit: query_limit,
            step: 1,
        }),
    }
    .encode(&mut query_bytes)
    .unwrap();

    // Setup mock DB executor and tell it to reply to the query
    let headers = vec![];
    let mut mock_db_executor = MockDBExecutor::default();
    mock_db_executor.query_to_headers.insert(query, headers);

    // Setup mock swarm and tell it to pole an event of new inbound query
    let mut mock_swarm = MockSwarm::default();
    let inbound_session_id = InboundSessionId { value: 0 };
    let _fut = mock_swarm.get_data_sent_to_inbound_session(inbound_session_id);
    mock_swarm.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
        mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::NewInboundSession {
            query: query_bytes,
            inbound_session_id,
            peer_id: PeerId::random(),
            protocol_name: crate::Protocol::SignedBlockHeader.into(),
        }),
    )));

    // Initiate swarm notifier to notify upon session closed
    let (inbound_session_closed_notifier, inbound_session_closed_receiver) = oneshot::channel();
    mock_swarm.inbound_session_closed_notifier = Some(inbound_session_closed_notifier);

    // Create network manager and run it
    let network_manager =
        GenericNetworkManager::generic_new(mock_swarm, mock_db_executor, BUFFER_SIZE);
    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        _ = inbound_session_closed_receiver => {}
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
}

#[tokio::test]
async fn broadcast_message() {
    let topic = Topic::new("TOPIC");
    let message = vec![1u8, 2u8, 3u8];

    let mut mock_swarm = MockSwarm::default();
    let mut messages_we_broadcasted_stream = mock_swarm.stream_messages_we_broadcasted();

    let mock_db_executor = MockDBExecutor::default();
    let mut network_manager =
        GenericNetworkManager::generic_new(mock_swarm, mock_db_executor, BUFFER_SIZE);

    let mut messages_to_broadcast_sender = network_manager
        .register_broadcast_subscriber(topic.clone(), BUFFER_SIZE)
        .unwrap()
        .messages_to_broadcast_sender;
    messages_to_broadcast_sender.send(message.clone()).await.unwrap();

    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            TIMEOUT, messages_we_broadcasted_stream.next()
        ) => {
            let (actual_message, topic_hash) = result.unwrap().unwrap();
            assert_eq!(message, actual_message);
            assert_eq!(topic.hash(), topic_hash);
        }
    }
}

#[tokio::test]
async fn receive_broadcasted_message_and_report_it() {
    let topic = Topic::new("TOPIC");
    let message = vec![1u8, 2u8, 3u8];
    let originated_peer_id = PeerId::random();

    let mut mock_swarm = MockSwarm::default();
    mock_swarm.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
        mixed_behaviour::ExternalEvent::GossipSub(gossipsub_impl::ExternalEvent::Received {
            originated_peer_id,
            message: message.clone(),
            topic_hash: topic.hash(),
        }),
    )));
    let mut reported_peer_receiver = mock_swarm.get_reported_peers_stream();

    let mock_db_executor = MockDBExecutor::default();
    let mut network_manager =
        GenericNetworkManager::generic_new(mock_swarm, mock_db_executor, BUFFER_SIZE);

    let mut broadcasted_messages_receiver = network_manager
        .register_broadcast_subscriber::<Bytes>(topic.clone(), BUFFER_SIZE)
        .unwrap()
        .broadcasted_messages_receiver;

    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        // We need to do the entire calculation in the future here so that the network will keep
        // running while we call report_callback.
        reported_peer_result = tokio::time::timeout(TIMEOUT, broadcasted_messages_receiver.next())
            .then(|result| {
                let (message_result, report_callback) = result.unwrap().unwrap();
                assert_eq!(message, message_result.unwrap());
                report_callback();
                tokio::time::timeout(TIMEOUT, reported_peer_receiver.next())
            }) => {
            assert_eq!(originated_peer_id, reported_peer_result.unwrap().unwrap());
        }
    }
}

fn get_test_connection_established_event(mock_peer_id: PeerId) -> Event {
    Event::ConnectionEstablished {
        peer_id: mock_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: ConnectedPoint::Dialer {
            address: Multiaddr::empty(),
            role_override: libp2p::core::Endpoint::Dialer,
        },
        num_established: std::num::NonZeroU32::new(1).unwrap(),
        concurrent_dial_errors: None,
        established_in: Duration::from_secs(0),
    }
}
