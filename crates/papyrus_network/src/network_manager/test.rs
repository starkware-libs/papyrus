use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use std::vec;

use deadqueue::unlimited::Queue;
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::channel::oneshot;
use futures::future::{poll_fn, FutureExt};
use futures::stream::Stream;
use futures::{pin_mut, Future, SinkExt, StreamExt};
use lazy_static::lazy_static;
use libp2p::core::ConnectedPoint;
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::sleep;

use super::swarm_trait::{Event, SwarmTrait};
use super::{GenericNetworkManager, SqmrSubscriberChannels};
use crate::gossipsub_impl::{self, Topic};
use crate::mixed_behaviour;
use crate::sqmr::behaviour::{PeerNotConnected, SessionIdNotFoundError};
use crate::sqmr::{Bytes, GenericEvent, InboundSessionId, OutboundSessionId};

const TIMEOUT: Duration = Duration::from_secs(1);

lazy_static! {
    static ref VEC1: Vec<u8> = vec![1, 2, 3, 4, 5];
    static ref VEC2: Vec<u8> = vec![6, 7, 8];
    static ref VEC3: Vec<u8> = vec![9, 10];
}

#[derive(Default)]
struct MockSwarm {
    pub pending_events: Queue<Event>,
    pub subscribed_topics: HashSet<TopicHash>,
    broadcasted_messages_senders: Vec<UnboundedSender<(Bytes, TopicHash)>>,
    reported_peer_senders: Vec<UnboundedSender<PeerId>>,
    supported_inbound_protocols_senders: Vec<UnboundedSender<StreamProtocol>>,
    inbound_session_id_to_response_sender: HashMap<InboundSessionId, UnboundedSender<Bytes>>,
    next_outbound_session_id: usize,
    first_polled_event_notifier: Option<oneshot::Sender<()>>,
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
    pub fn get_responses_sent_to_inbound_session(
        &mut self,
        inbound_session_id: InboundSessionId,
    ) -> impl Future<Output = Vec<Bytes>> {
        let (responses_sender, responses_receiver) = unbounded();
        if self
            .inbound_session_id_to_response_sender
            .insert(inbound_session_id, responses_sender)
            .is_some()
        {
            panic!("Called get_responses_sent_to_inbound_session on {inbound_session_id:?} twice");
        }
        responses_receiver.collect()
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

    pub fn get_supported_inbound_protocol(&mut self) -> impl Stream<Item = StreamProtocol> {
        let (sender, receiver) = unbounded();
        self.supported_inbound_protocols_senders.push(sender);
        receiver
    }

    fn create_response_events_for_query_each_num_becomes_response(
        &self,
        query: Vec<u8>,
        outbound_session_id: OutboundSessionId,
        peer_id: PeerId,
    ) {
        for response in query {
            self.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
                mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::ReceivedResponse {
                    response: vec![response],
                    outbound_session_id,
                    peer_id,
                }),
            )));
        }
    }
}

impl SwarmTrait for MockSwarm {
    fn send_response(
        &mut self,
        response: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let responses_sender =
            self.inbound_session_id_to_response_sender.get(&inbound_session_id).expect(
                "Called send_response without calling get_responses_sent_to_inbound_session first",
            );
        responses_sender.unbounded_send(response).unwrap();
        Ok(())
    }

    fn send_query(
        &mut self,
        query: Vec<u8>,
        peer_id: PeerId,
        _protocol: StreamProtocol,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        let outbound_session_id = OutboundSessionId { value: self.next_outbound_session_id };
        self.create_response_events_for_query_each_num_becomes_response(
            query,
            outbound_session_id,
            peer_id,
        );
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
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let responses_sender =
            self.inbound_session_id_to_response_sender.get(&inbound_session_id).expect(
                "Called close_inbound_session without calling \
                 get_responses_sent_to_inbound_session first",
            );
        responses_sender.close_channel();
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
    fn add_new_supported_inbound_protocol(&mut self, protocol_name: StreamProtocol) {
        for sender in &self.supported_inbound_protocols_senders {
            sender.unbounded_send(protocol_name.clone()).unwrap();
        }
    }
}

const BUFFER_SIZE: usize = 100;
const SIGNED_BLOCK_HEADER_PROTOCOL: StreamProtocol = StreamProtocol::new("/starknet/headers/1");

#[tokio::test]
async fn register_sqmr_protocol_client_and_use_channels() {
    // mock swarm to send and track connection established event
    let mut mock_swarm = MockSwarm::default();
    let peer_id = PeerId::random();
    mock_swarm.pending_events.push(get_test_connection_established_event(peer_id));
    let (event_notifier, mut event_listner) = oneshot::channel();
    mock_swarm.first_polled_event_notifier = Some(event_notifier);

    // network manager to register subscriber and send query
    let mut network_manager = GenericNetworkManager::generic_new(mock_swarm);

    // register subscriber and send query
    let SqmrSubscriberChannels { mut query_sender, response_receiver } = network_manager
        .register_sqmr_protocol_client::<Vec<u8>, Vec<u8>>(
            SIGNED_BLOCK_HEADER_PROTOCOL.to_string(),
            BUFFER_SIZE,
        );

    let response_receiver_length = Arc::new(Mutex::new(0));
    let cloned_response_receiver_length = Arc::clone(&response_receiver_length);
    let response_receiver_collector = response_receiver
        .enumerate()
        .take(VEC1.len())
        .map(|(i, (result, _report_callback))| {
            let result = result.unwrap();
            // this simulates how the mock swarm parses the query and sends responses to it
            assert_eq!(result, vec![VEC1[i]]);
            result
        })
        .collect::<Vec<_>>();
    tokio::select! {
        _ = network_manager.run() => panic!("network manager ended"),
        _ = poll_fn(|cx| event_listner.poll_unpin(cx)).then(|_| async move {
            query_sender.send(VEC1.clone()).await.unwrap()})
            .then(|_| async move {
                *cloned_response_receiver_length.lock().await = response_receiver_collector.await.len();
            }) => {},
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Test timed out");
        }
    }
    assert_eq!(*response_receiver_length.lock().await, VEC1.len());
}

// TODO(shahak): Add multiple protocols and multiple queries in the test.
#[tokio::test]
async fn process_incoming_query() {
    // Create responses for test.
    let query = VEC1.clone();
    let responses = vec![VEC1.clone(), VEC2.clone(), VEC3.clone()];
    let protocol: StreamProtocol = SIGNED_BLOCK_HEADER_PROTOCOL;

    // Setup mock swarm and tell it to return an event of new inbound query.
    let mut mock_swarm = MockSwarm::default();
    let inbound_session_id = InboundSessionId { value: 0 };
    mock_swarm.pending_events.push(Event::Behaviour(mixed_behaviour::Event::ExternalEvent(
        mixed_behaviour::ExternalEvent::Sqmr(GenericEvent::NewInboundSession {
            query: query.clone(),
            inbound_session_id,
            peer_id: PeerId::random(),
            protocol_name: protocol.clone(),
        }),
    )));

    // Create a future that will return when the session is closed with the responses sent on the
    // swarm.
    let get_responses_fut = mock_swarm.get_responses_sent_to_inbound_session(inbound_session_id);
    let mut get_supported_inbound_protocol_fut = mock_swarm.get_supported_inbound_protocol();

    let mut network_manager = GenericNetworkManager::generic_new(mock_swarm);

    let mut inbound_query_receiver = network_manager
        .register_sqmr_protocol_server::<Vec<u8>, Vec<u8>>(protocol.to_string(), BUFFER_SIZE);

    let actual_protocol = get_supported_inbound_protocol_fut.next().await.unwrap();
    assert_eq!(protocol, actual_protocol);

    let responses_clone = responses.clone();
    select! {
        _ = async move {
            let (query_got, mut responses_sender) = inbound_query_receiver.next().await.unwrap();
            assert_eq!(query_got.unwrap(), query);
            for response in responses_clone {
                responses_sender.feed(response).await.unwrap();
            }
            responses_sender.close().await.unwrap();
            assert_eq!(get_responses_fut.await, responses);
        } => {}
        _ = network_manager.run() => {
            panic!("GenericNetworkManager::run finished before the session finished");
        }
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

    let mut network_manager = GenericNetworkManager::generic_new(mock_swarm);

    let mut messages_to_broadcast_sender = network_manager
        .register_broadcast_topic(topic.clone(), BUFFER_SIZE)
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

    let mut network_manager = GenericNetworkManager::generic_new(mock_swarm);

    let mut broadcasted_messages_receiver = network_manager
        .register_broadcast_topic::<Bytes>(topic.clone(), BUFFER_SIZE)
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
