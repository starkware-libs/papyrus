mod swarm_trait;

#[cfg(test)]
mod test;

use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::channel::oneshot;
use futures::future::{ready, BoxFuture, Ready};
use futures::sink::With;
use futures::stream::{self, BoxStream, FuturesUnordered, Map};
use futures::{FutureExt, SinkExt, StreamExt};
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, StreamProtocol, Swarm};
use metrics::gauge;
use papyrus_common::metrics as papyrus_metrics;
use sqmr::Bytes;
use tracing::{debug, error, info, trace};

use self::swarm_trait::SwarmTrait;
use crate::bin_utils::build_swarm;
use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::{self, BridgedBehaviour};
use crate::sqmr::{self, InboundSessionId, OutboundSessionId, SessionId};
use crate::utils::StreamHashMap;
use crate::{gossipsub_impl, NetworkConfig};

#[derive(thiserror::Error, Debug)]
pub enum NetworkError {
    #[error(transparent)]
    DialError(#[from] libp2p::swarm::DialError),
}

pub struct GenericNetworkManager<SwarmT: SwarmTrait> {
    swarm: SwarmT,
    inbound_protocol_to_buffer_size: HashMap<StreamProtocol, usize>,
    sqmr_inbound_response_receivers:
        StreamHashMap<InboundSessionId, BoxStream<'static, Option<Bytes>>>,
    sqmr_inbound_query_senders: HashMap<StreamProtocol, Sender<(Bytes, Sender<Bytes>)>>,
    // Splitting the response receivers from the query senders in order to poll all
    // receivers simultaneously.
    // Each receiver has a matching sender and vice versa (i.e the maps have the same keys).
    sqmr_outbound_query_receivers: StreamHashMap<StreamProtocol, Receiver<Bytes>>,
    sqmr_outbound_response_senders: HashMap<StreamProtocol, Sender<(Bytes, ReportCallback)>>,
    // Splitting the broadcast receivers from the broadcasted senders in order to poll all
    // receivers simultaneously.
    // Each receiver has a matching sender and vice versa (i.e the maps have the same keys).
    messages_to_broadcast_receivers: StreamHashMap<TopicHash, Receiver<Bytes>>,
    broadcasted_messages_senders: HashMap<TopicHash, Sender<(Bytes, ReportCallback)>>,
    outbound_session_id_to_protocol: HashMap<OutboundSessionId, StreamProtocol>,
    reported_peer_receivers: FuturesUnordered<BoxFuture<'static, Option<PeerId>>>,
    // Fields for metrics
    num_active_inbound_sessions: usize,
    num_active_outbound_sessions: usize,
}

impl<SwarmT: SwarmTrait> GenericNetworkManager<SwarmT> {
    pub async fn run(mut self) -> Result<(), NetworkError> {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.sqmr_inbound_response_receivers.next() => self.handle_response_for_inbound_query(res),
                Some((protocol, query)) = self.sqmr_outbound_query_receivers.next() => {
                    self.handle_local_sqmr_query(protocol, query)
                }
                Some((topic_hash, message)) = self.messages_to_broadcast_receivers.next() => {
                    self.broadcast_message(message, topic_hash);
                }
                Some(Some(peer_id)) = self.reported_peer_receivers.next() => self.swarm.report_peer(peer_id),
            }
        }
    }

    pub(crate) fn generic_new(swarm: SwarmT) -> Self {
        gauge!(papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS, 0f64);
        let reported_peer_receivers = FuturesUnordered::new();
        reported_peer_receivers.push(futures::future::pending().boxed());
        Self {
            swarm,
            inbound_protocol_to_buffer_size: HashMap::new(),
            sqmr_inbound_response_receivers: StreamHashMap::new(HashMap::new()),
            sqmr_inbound_query_senders: HashMap::new(),
            sqmr_outbound_query_receivers: StreamHashMap::new(HashMap::new()),
            sqmr_outbound_response_senders: HashMap::new(),
            messages_to_broadcast_receivers: StreamHashMap::new(HashMap::new()),
            broadcasted_messages_senders: HashMap::new(),
            outbound_session_id_to_protocol: HashMap::new(),
            reported_peer_receivers,
            num_active_inbound_sessions: 0,
            num_active_outbound_sessions: 0,
        }
    }

    /// TODO: Support multiple protocols where they're all different versions of the same protocol
    pub fn register_sqmr_protocol_server<Query, Response>(
        &mut self,
        protocol: String,
        buffer_size: usize,
    ) -> SqmrQueryReceiver<Query, Response>
    where
        Bytes: From<Response>,
        Query: TryFrom<Bytes>,
    {
        let protocol = StreamProtocol::try_from_owned(protocol)
            .expect("Could not parse protocol into StreamProtocol.");
        self.swarm.add_new_supported_inbound_protocol(protocol.clone());
        if let Some(_old_buffer_size) =
            self.inbound_protocol_to_buffer_size.insert(protocol.clone(), buffer_size)
        {
            panic!("Protocol '{}' has already been registered as a server.", protocol);
        }
        let (inbound_query_sender, inbound_query_receiver) =
            futures::channel::mpsc::channel(buffer_size);
        let result = self.sqmr_inbound_query_senders.insert(protocol.clone(), inbound_query_sender);
        if result.is_some() {
            panic!("Protocol '{}' has already been registered as a server.", protocol);
        }

        inbound_query_receiver.map(|(query_bytes, response_bytes_sender)| {
            (
                Query::try_from(query_bytes),
                response_bytes_sender.with(|response| ready(Ok(Bytes::from(response)))),
            )
        })
    }

    /// TODO: Support multiple protocols where they're all different versions of the same protocol
    /// Register a new subscriber for sending a single query and receiving multiple responses.
    /// Panics if the given protocol is already subscribed.
    pub fn register_sqmr_protocol_client<Query, Response>(
        &mut self,
        protocol: String,
        buffer_size: usize,
    ) -> SqmrSubscriberChannels<Query, Response>
    where
        Bytes: From<Query>,
        Response: TryFrom<Bytes>,
    {
        let protocol = StreamProtocol::try_from_owned(protocol)
            .expect("Could not parse protocol into StreamProtocol.");
        self.swarm.add_new_supported_inbound_protocol(protocol.clone());
        let (query_sender, query_receiver) = futures::channel::mpsc::channel(buffer_size);
        let (response_sender, response_receiver) = futures::channel::mpsc::channel(buffer_size);

        let insert_result =
            self.sqmr_outbound_query_receivers.insert(protocol.clone(), query_receiver);
        if insert_result.is_some() {
            panic!("Protocol '{}' has already been registered as a client.", protocol);
        }
        let insert_result =
            self.sqmr_outbound_response_senders.insert(protocol.clone(), response_sender);
        if insert_result.is_some() {
            panic!("Protocol '{}' has already been registered as a client.", protocol);
        }

        let query_fn: fn(Query) -> Ready<Result<Bytes, SendError>> =
            |query| ready(Ok(Bytes::from(query)));
        let query_sender = query_sender.with(query_fn);

        let response_fn: ReceivedMessagesConverterFn<Response> =
            |(x, report_callback)| (Response::try_from(x), report_callback);
        let response_receiver = response_receiver.map(response_fn);

        SqmrSubscriberChannels { query_sender, response_receiver }
    }

    /// Register a new subscriber for broadcasting and receiving broadcasts for a given topic.
    /// Panics if this topic is already subscribed.
    pub fn register_broadcast_topic<T>(
        &mut self,
        topic: Topic,
        buffer_size: usize,
    ) -> Result<BroadcastSubscriberChannels<T>, SubscriptionError>
    where
        T: TryFrom<Bytes>,
        Bytes: From<T>,
    {
        self.swarm.subscribe_to_topic(&topic)?;

        let topic_hash = topic.hash();

        let (messages_to_broadcast_sender, messages_to_broadcast_receiver) =
            futures::channel::mpsc::channel(buffer_size);
        let (broadcasted_messages_sender, broadcasted_messages_receiver) =
            futures::channel::mpsc::channel(buffer_size);

        let insert_result = self
            .messages_to_broadcast_receivers
            .insert(topic_hash.clone(), messages_to_broadcast_receiver);
        if insert_result.is_some() {
            panic!("Topic '{}' has already been registered.", topic);
        }

        let insert_result = self
            .broadcasted_messages_senders
            .insert(topic_hash.clone(), broadcasted_messages_sender);
        if insert_result.is_some() {
            panic!("Topic '{}' has already been registered.", topic);
        }

        let messages_to_broadcast_fn: fn(T) -> Ready<Result<Bytes, SendError>> =
            |x| ready(Ok(Bytes::from(x)));
        let messages_to_broadcast_sender =
            messages_to_broadcast_sender.with(messages_to_broadcast_fn);

        let broadcasted_messages_fn: ReceivedMessagesConverterFn<T> =
            |(x, report_callback)| (T::try_from(x), report_callback);
        let broadcasted_messages_receiver =
            broadcasted_messages_receiver.map(broadcasted_messages_fn);

        Ok(BroadcastSubscriberChannels {
            messages_to_broadcast_sender,
            broadcasted_messages_receiver,
        })
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<mixed_behaviour::Event>) {
        if let SwarmEvent::Behaviour(mixed_behaviour::Event::ToOtherBehaviourEvent(
            mixed_behaviour::ToOtherBehaviourEvent::NoOp,
        )) = event
        {
        } else {
            info!("ASMAA -- Received swarm event: {event:?}");
        }

        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                debug!("Connected to peer id: {peer_id:?}");
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS,
                    self.swarm.num_connected_peers() as f64
                );
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                match cause {
                    Some(connection_error) => {
                        debug!("Connection to {peer_id:?} closed due to {connection_error:?}.")
                    }
                    None => debug!("Connection to {peer_id:?} closed."),
                }
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS,
                    self.swarm.num_connected_peers() as f64
                );
            }
            SwarmEvent::Behaviour(event) => {
                self.handle_behaviour_event(event);
            }
            SwarmEvent::OutgoingConnectionError { connection_id, peer_id, error } => {
                error!(
                    "Outgoing connection error. connection id: {connection_id:?}, requested peer \
                     id: {peer_id:?}, error: {error:?}"
                );
            }
            SwarmEvent::IncomingConnectionError {
                connection_id,
                local_addr,
                send_back_addr,
                error,
            } => {
                // No need to panic here since this is a result of another peer trying to dial to us
                // and failing. Other peers are welcome to retry.
                error!(
                    "Incoming connection error. connection id: {connection_id:?}, local addr: \
                     {local_addr:?}, send back addr: {send_back_addr:?}, error: {error:?}"
                );
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                // TODO(shahak): Once we support nodes behind a NAT, fix this to only add external
                // addresses.
                self.swarm.add_external_address(address);
            }
            SwarmEvent::IncomingConnection { .. }
            | SwarmEvent::Dialing { .. }
            | SwarmEvent::NewExternalAddrCandidate { .. } => {}
            _ => {
                error!("Unexpected event {event:?}");
            }
        }
    }

    fn handle_behaviour_event(&mut self, event: mixed_behaviour::Event) {
        match event {
            mixed_behaviour::Event::ExternalEvent(external_event) => {
                self.handle_behaviour_external_event(external_event);
            }
            mixed_behaviour::Event::ToOtherBehaviourEvent(internal_event) => {
                self.handle_to_other_behaviour_event(internal_event);
            }
        }
    }

    fn handle_behaviour_external_event(&mut self, event: mixed_behaviour::ExternalEvent) {
        match event {
            mixed_behaviour::ExternalEvent::Sqmr(event) => {
                self.handle_sqmr_behaviour_event(event);
            }
            mixed_behaviour::ExternalEvent::GossipSub(event) => {
                self.handle_gossipsub_behaviour_event(event);
            }
        }
    }

    fn handle_to_other_behaviour_event(&mut self, event: mixed_behaviour::ToOtherBehaviourEvent) {
        // TODO(shahak): Move this logic to mixed_behaviour.
        if let mixed_behaviour::ToOtherBehaviourEvent::NoOp = event {
            return;
        }
        self.swarm.behaviour_mut().identify.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().kademlia.on_other_behaviour_event(&event);
        if let Some(discovery) = self.swarm.behaviour_mut().discovery.as_mut() {
            discovery.on_other_behaviour_event(&event);
        }
        self.swarm.behaviour_mut().sqmr.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().peer_manager.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().gossipsub.on_other_behaviour_event(&event);
    }

    fn handle_sqmr_behaviour_event(&mut self, event: sqmr::behaviour::ExternalEvent) {
        // TODO(shahak): Extract the body of each match arm to a separate function.
        match event {
            sqmr::behaviour::ExternalEvent::NewInboundSession {
                query,
                inbound_session_id,
                peer_id: _,
                protocol_name,
            } => {
                info!(
                    "Received new inbound query: {query:?} for session id: {inbound_session_id:?}"
                );
                self.num_active_inbound_sessions += 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_INBOUND_SESSIONS,
                    self.num_active_inbound_sessions as f64
                );
                // TODO: consider returning error instead of panic.
                let Some(query_sender) = self.sqmr_inbound_query_senders.get_mut(&protocol_name)
                else {
                    return;
                };
                let (response_sender, response_receiver) = futures::channel::mpsc::channel(
                    *self.inbound_protocol_to_buffer_size.get(&protocol_name).expect(
                        "A protocol is registered in NetworkManager but it has no buffer size.",
                    ),
                );
                // TODO(shahak): Close the inbound session if the buffer is full.
                send_now(
                    query_sender,
                    (query, response_sender),
                    format!(
                        "Received an inbound query while the buffer is full. Dropping query for \
                         session {inbound_session_id:?}"
                    ),
                );
                self.sqmr_inbound_response_receivers.insert(
                    inbound_session_id,
                    response_receiver.map(Some).chain(stream::once(ready(None))).boxed(),
                );
            }
            sqmr::behaviour::ExternalEvent::ReceivedResponse {
                outbound_session_id,
                response,
                peer_id,
            } => {
                trace!(
                    "Received response from peer for session id: {outbound_session_id:?}. sending \
                     to sync subscriber."
                );
                let protocol = self
                    .outbound_session_id_to_protocol
                    .get(&outbound_session_id)
                    .expect("Received response from an unknown session id");
                let report_callback_sender =
                    self.create_external_callback_for_received_data(peer_id);
                if let Some(response_sender) = self.sqmr_outbound_response_senders.get_mut(protocol)
                {
                    // TODO(shahak): Close the channel if the buffer is full.
                    send_now(
                        response_sender,
                        (response, report_callback_sender),
                        format!(
                            "Received response for an outbound query while the buffer is full. \
                             Dropping it. Session: {outbound_session_id:?}"
                        ),
                    );
                }
            }
            sqmr::behaviour::ExternalEvent::SessionFailed { session_id, error } => {
                error!("Session {session_id:?} failed on {error:?}");
                self.report_session_removed_to_metrics(session_id);
                // TODO: Handle reputation and retry.
                if let SessionId::OutboundSessionId(outbound_session_id) = session_id {
                    self.outbound_session_id_to_protocol.remove(&outbound_session_id);
                }
            }
            sqmr::behaviour::ExternalEvent::SessionFinishedSuccessfully { session_id } => {
                debug!("Session completed successfully. session_id: {session_id:?}");
                self.report_session_removed_to_metrics(session_id);
                if let SessionId::OutboundSessionId(outbound_session_id) = session_id {
                    self.outbound_session_id_to_protocol.remove(&outbound_session_id);
                }
            }
        }
    }

    fn handle_gossipsub_behaviour_event(&mut self, event: gossipsub_impl::ExternalEvent) {
        match event {
            gossipsub_impl::ExternalEvent::Received { originated_peer_id, message, topic_hash } => {
                let report_callback_sender =
                    self.create_external_callback_for_received_data(originated_peer_id);
                let Some(sender) = self.broadcasted_messages_senders.get_mut(&topic_hash) else {
                    error!(
                        "Received a message from a topic we're not subscribed to with hash \
                         {topic_hash:?}"
                    );
                    return;
                };
                let send_result = sender.try_send((message, report_callback_sender));
                if let Err(e) = send_result {
                    if e.is_disconnected() {
                        panic!("Receiver was dropped. This should never happen.")
                    } else if e.is_full() {
                        error!(
                            "Receiver buffer is full. Dropping broadcasted message for topic with \
                             hash: {topic_hash:?}."
                        );
                    }
                }
            }
        }
    }

    fn handle_response_for_inbound_query(&mut self, res: (InboundSessionId, Option<Bytes>)) {
        let (inbound_session_id, maybe_response) = res;
        match maybe_response {
            Some(response) => {
                self.swarm.send_response(response, inbound_session_id).unwrap_or_else(|e| {
                    error!(
                        "Failed to send response to peer. Session id: {inbound_session_id:?} not \
                         found error: {e:?}"
                    );
                });
            }
            None => {
                self.swarm.close_inbound_session(inbound_session_id).unwrap_or_else(|e| {
                    error!(
                        "Failed to close session after sending all response. Session id: \
                         {inbound_session_id:?} not found error: {e:?}"
                    )
                });
            }
        };
    }

    fn handle_local_sqmr_query(&mut self, protocol: StreamProtocol, query: Bytes) {
        match self.swarm.send_query(query, PeerId::random(), protocol.clone()) {
            Ok(outbound_session_id) => {
                debug!("Sent query to peer. outbound_session_id: {outbound_session_id:?}");
                self.num_active_outbound_sessions += 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_OUTBOUND_SESSIONS,
                    self.num_active_outbound_sessions as f64
                );
                self.outbound_session_id_to_protocol.insert(outbound_session_id, protocol);
            }
            Err(e) => {
                info!(
                    "Failed to send query to peer. Peer not connected error: {e:?} Returning \
                     empty response to sync subscriber."
                );
            }
        }
    }

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash) {
        self.swarm.broadcast_message(message, topic_hash);
    }

    fn report_session_removed_to_metrics(&mut self, session_id: SessionId) {
        match session_id {
            SessionId::InboundSessionId(_) => {
                self.num_active_inbound_sessions -= 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_INBOUND_SESSIONS,
                    self.num_active_inbound_sessions as f64
                );
            }
            SessionId::OutboundSessionId(_) => {
                self.num_active_outbound_sessions += 1;
                gauge!(
                    papyrus_metrics::PAPYRUS_NUM_ACTIVE_OUTBOUND_SESSIONS,
                    self.num_active_outbound_sessions as f64
                );
            }
        }
    }
    fn create_external_callback_for_received_data(
        &self,
        peer_id: PeerId,
    ) -> Box<dyn FnOnce() + Send> {
        let (report_callback_sender, report_callback_receiver) = oneshot::channel::<()>();
        self.reported_peer_receivers.push(
            report_callback_receiver
                .map(move |result| match result {
                    Ok(_) => Some(peer_id),
                    Err(_) => None,
                })
                .boxed(),
        );
        Box::new(move || {
            // TODO(shahak): Check if we can panic in case of error.
            let _ = report_callback_sender.send(());
        })
    }
}

pub type NetworkManager = GenericNetworkManager<Swarm<mixed_behaviour::MixedBehaviour>>;

impl NetworkManager {
    pub fn new(config: NetworkConfig) -> Self {
        let NetworkConfig {
            tcp_port,
            quic_port: _,
            session_timeout,
            idle_connection_timeout,
            bootstrap_peer_multiaddr,
            secret_key,
        } = config;

        let listen_addresses = vec![
            // TODO: uncomment once quic transpot works.
            // format!("/ip4/0.0.0.0/udp/{quic_port}/quic-v1"),
            format!("/ip4/0.0.0.0/tcp/{tcp_port}"),
        ];
        let swarm = build_swarm(listen_addresses, idle_connection_timeout, secret_key, |key| {
            mixed_behaviour::MixedBehaviour::new(
                key,
                bootstrap_peer_multiaddr.clone(),
                sqmr::Config { session_timeout },
            )
        });
        Self::generic_new(swarm)
    }

    pub fn get_local_peer_id(&self) -> String {
        self.swarm.local_peer_id().to_string()
    }
}

#[cfg(feature = "testing")]
const CHANNEL_BUFFER_SIZE: usize = 1000;

#[cfg(feature = "testing")]
pub fn mock_register_broadcast_subscriber<T>()
-> Result<TestSubscriberChannels<T>, SubscriptionError>
where
    T: TryFrom<Bytes>,
    Bytes: From<T>,
{
    let (messages_to_broadcast_sender, mock_messages_to_broadcast_receiver) =
        futures::channel::mpsc::channel(CHANNEL_BUFFER_SIZE);
    let (mock_broadcasted_messages_sender, broadcasted_messages_receiver) =
        futures::channel::mpsc::channel(CHANNEL_BUFFER_SIZE);

    let messages_to_broadcast_fn: fn(T) -> Ready<Result<Bytes, SendError>> =
        |x| ready(Ok(Bytes::from(x)));
    let messages_to_broadcast_sender = messages_to_broadcast_sender.with(messages_to_broadcast_fn);

    let broadcasted_messages_fn: ReceivedMessagesConverterFn<T> =
        |(x, report_callback)| (T::try_from(x), report_callback);
    let broadcasted_messages_receiver = broadcasted_messages_receiver.map(broadcasted_messages_fn);

    let subscriber_channels =
        BroadcastSubscriberChannels { messages_to_broadcast_sender, broadcasted_messages_receiver };

    let mock_broadcasted_messages_fn: MockBroadcastedMessagesFn<T> =
        |(x, report_call_back)| ready(Ok((Bytes::from(x), report_call_back)));
    let mock_broadcasted_messages_sender =
        mock_broadcasted_messages_sender.with(mock_broadcasted_messages_fn);

    let mock_messages_to_broadcast_fn: fn(Bytes) -> T = |x| match T::try_from(x) {
        Ok(result) => result,
        Err(_) => {
            panic!("Failed to convert Bytes that we received from conversion to bytes");
        }
    };
    let mock_messages_to_broadcast_receiver =
        mock_messages_to_broadcast_receiver.map(mock_messages_to_broadcast_fn);

    let mock_network = BroadcastNetworkMock {
        broadcasted_messages_sender: mock_broadcasted_messages_sender,
        messages_to_broadcast_receiver: mock_messages_to_broadcast_receiver,
    };

    Ok(TestSubscriberChannels { subscriber_channels, mock_network })
}

#[cfg(feature = "testing")]
pub fn dummy_report_callback() -> ReportCallback {
    Box::new(|| {})
}

// TODO(shahak): Create a custom struct if Box dyn becomes an overhead.
pub type ReportCallback = Box<dyn FnOnce() + Send>;

// TODO(shahak): Add report callback.
pub type SqmrQueryReceiver<Query, Response> =
    Map<Receiver<(Bytes, Sender<Bytes>)>, ReceivedQueryConverterFn<Query, Response>>;

type ReceivedQueryConverterFn<Query, Response> =
    fn(
        (Bytes, Sender<Bytes>),
    ) -> (Result<Query, <Query as TryFrom<Bytes>>::Error>, SubscriberSender<Response>);

pub type SubscriberSender<T> = With<
    Sender<Bytes>,
    Bytes,
    T,
    Ready<Result<Bytes, SendError>>,
    fn(T) -> Ready<Result<Bytes, SendError>>,
>;

// TODO(shahak): rename to ConvertFromBytesReceiver and add an alias called BroadcastReceiver
pub type SubscriberReceiver<T> =
    Map<Receiver<(Bytes, ReportCallback)>, ReceivedMessagesConverterFn<T>>;

type ReceivedMessagesConverterFn<T> =
    fn((Bytes, ReportCallback)) -> (Result<T, <T as TryFrom<Bytes>>::Error>, ReportCallback);

// TODO(shahak): Unite channels to a Sender of Query and Receiver of Responses.
pub struct SqmrSubscriberChannels<Query: Into<Bytes>, Response: TryFrom<Bytes>> {
    pub query_sender: SubscriberSender<Query>,
    pub response_receiver: SubscriberReceiver<Response>,
}

pub struct BroadcastSubscriberChannels<T: TryFrom<Bytes>> {
    pub messages_to_broadcast_sender: SubscriberSender<T>,
    pub broadcasted_messages_receiver: SubscriberReceiver<T>,
}

#[cfg(feature = "testing")]
pub type MockBroadcastedMessagesSender<T> = With<
    Sender<(Bytes, ReportCallback)>,
    (Bytes, ReportCallback),
    (T, ReportCallback),
    Ready<Result<(Bytes, ReportCallback), SendError>>,
    MockBroadcastedMessagesFn<T>,
>;
#[cfg(feature = "testing")]
type MockBroadcastedMessagesFn<T> =
    fn((T, ReportCallback)) -> Ready<Result<(Bytes, ReportCallback), SendError>>;
#[cfg(feature = "testing")]
pub type MockMessagesToBroadcastReceiver<T> = Map<Receiver<Bytes>, fn(Bytes) -> T>;
#[cfg(feature = "testing")]
pub struct BroadcastNetworkMock<T: TryFrom<Bytes>> {
    pub broadcasted_messages_sender: MockBroadcastedMessagesSender<T>,
    pub messages_to_broadcast_receiver: MockMessagesToBroadcastReceiver<T>,
}
#[cfg(feature = "testing")]
pub struct TestSubscriberChannels<T: TryFrom<Bytes>> {
    pub subscriber_channels: BroadcastSubscriberChannels<T>,
    pub mock_network: BroadcastNetworkMock<T>,
}

fn send_now<Item>(sender: &mut Sender<Item>, item: Item, buffer_full_message: String) {
    if let Err(error) = sender.try_send(item) {
        if error.is_disconnected() {
            panic!("Receiver was dropped. This should never happen.")
        } else if error.is_full() {
            // TODO(shahak): Consider doing something else rather than dropping the message.
            error!(buffer_full_message);
        }
    }
}
