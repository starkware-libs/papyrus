mod swarm_trait;

#[cfg(test)]
mod test;

use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, SendError, Sender, UnboundedReceiver, UnboundedSender};
use futures::future::{pending, ready, Ready};
use futures::sink::With;
use futures::stream::{self, BoxStream, Map, SelectAll};
use futures::{FutureExt, SinkExt, StreamExt};
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm};
use metrics::gauge;
use papyrus_common::metrics as papyrus_metrics;
use papyrus_storage::StorageReader;
use streamed_bytes::Bytes;
use tracing::{debug, error, info, trace};

use self::swarm_trait::SwarmTrait;
use crate::bin_utils::build_swarm;
use crate::converters::{Router, RouterError};
use crate::db_executor::{self, BlockHeaderDBExecutor, DBExecutor, Data, QueryId};
use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::{self, BridgedBehaviour};
use crate::streamed_bytes::{self, InboundSessionId, OutboundSessionId, SessionId};
use crate::utils::StreamHashMap;
use crate::{gossipsub_impl, DataType, NetworkConfig, Protocol, Query, ResponseReceivers};

type StreamCollection = SelectAll<BoxStream<'static, (Data, InboundSessionId)>>;
type SubscriberChannels = (Receiver<Query>, Router);

#[derive(thiserror::Error, Debug)]
pub enum NetworkError {
    #[error(transparent)]
    DialError(#[from] libp2p::swarm::DialError),
}

pub struct GenericNetworkManager<DBExecutorT: DBExecutor, SwarmT: SwarmTrait> {
    swarm: SwarmT,
    db_executor: DBExecutorT,
    header_buffer_size: usize,
    query_results_router: StreamCollection,
    sync_subscriber_channels: Option<SubscriberChannels>,
    messages_to_broadcast_receivers: StreamHashMap<TopicHash, Receiver<Bytes>>,
    broadcasted_messages_senders: HashMap<TopicHash, Sender<(Bytes, ReportCallback)>>,
    query_id_to_inbound_session_id: HashMap<QueryId, InboundSessionId>,
    outbound_session_id_to_protocol: HashMap<OutboundSessionId, Protocol>,
    reported_peer_receiver: UnboundedReceiver<PeerId>,
    // We keep this just for giving a clone of it for subscribers.
    reported_peer_sender: UnboundedSender<PeerId>,
    // Fields for metrics
    num_active_inbound_sessions: usize,
    num_active_outbound_sessions: usize,
}

impl<DBExecutorT: DBExecutor, SwarmT: SwarmTrait> GenericNetworkManager<DBExecutorT, SwarmT> {
    pub async fn run(mut self) -> Result<(), NetworkError> {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.db_executor.next() => self.handle_db_executor_result(res),
                Some(res) = self.query_results_router.next() => self.handle_query_result_routing_to_other_peer(res),
                Some(res) = self.sync_subscriber_channels.as_mut()
                .map(|(query_receiver, _)| query_receiver.next().boxed())
                .unwrap_or(pending().boxed()) => self.handle_sync_subscriber_query(res),
                Some((topic_hash, message)) = self.messages_to_broadcast_receivers.next() => self.broadcast_message(message, topic_hash),
                Some(peer_id) = self.reported_peer_receiver.next() => self.swarm.report_peer(peer_id),
            }
        }
    }

    pub(crate) fn generic_new(
        swarm: SwarmT,
        db_executor: DBExecutorT,
        header_buffer_size: usize,
    ) -> Self {
        gauge!(papyrus_metrics::PAPYRUS_NUM_CONNECTED_PEERS, 0f64);
        let (reported_peer_sender, reported_peer_receiver) = futures::channel::mpsc::unbounded();
        Self {
            swarm,
            db_executor,
            header_buffer_size,
            query_results_router: StreamCollection::new(),
            sync_subscriber_channels: None,
            messages_to_broadcast_receivers: StreamHashMap::new(HashMap::new()),
            broadcasted_messages_senders: HashMap::new(),
            query_id_to_inbound_session_id: HashMap::new(),
            outbound_session_id_to_protocol: HashMap::new(),
            reported_peer_sender,
            reported_peer_receiver,
            num_active_inbound_sessions: 0,
            num_active_outbound_sessions: 0,
        }
    }

    pub fn register_subscriber(
        &mut self,
        protocols: Vec<Protocol>,
    ) -> (Sender<Query>, ResponseReceivers) {
        let (sender, query_receiver) = futures::channel::mpsc::channel(self.header_buffer_size);
        let mut router = Router::new(protocols, self.header_buffer_size);
        let response_receiver = ResponseReceivers::new(router.get_recievers());
        self.sync_subscriber_channels = Some((query_receiver, router));
        (sender, response_receiver)
    }

    /// Register a new subscriber for broadcasting and receiving broadcasts for a given topic.
    /// Panics if this topic is already subscribed.
    pub fn register_broadcast_subscriber<T>(
        &mut self,
        topic: Topic,
        buffer_size: usize,
    ) -> Result<BroadcastSubscriberChannels<T, <T as TryFrom<Bytes>>::Error>, SubscriptionError>
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
            panic!("Topic {} was registered twice", topic);
        }

        let insert_result = self
            .broadcasted_messages_senders
            .insert(topic_hash.clone(), broadcasted_messages_sender);
        if insert_result.is_some() {
            panic!("Topic {} was registered twice", topic);
        }

        let messages_to_broadcast_fn: fn(T) -> Ready<Result<Bytes, SendError>> =
            |x| ready(Ok(Bytes::from(x)));
        let messages_to_broadcast_sender =
            messages_to_broadcast_sender.with(messages_to_broadcast_fn);

        let broadcasted_messages_fn: fn(
            (Bytes, ReportCallback),
        ) -> (
            Result<T, <T as TryFrom<Bytes>>::Error>,
            ReportCallback,
        ) = |(x, report_callback)| (T::try_from(x), report_callback);
        let broadcasted_messages_receiver =
            broadcasted_messages_receiver.map(broadcasted_messages_fn);

        Ok(BroadcastSubscriberChannels {
            messages_to_broadcast_sender,
            broadcasted_messages_receiver,
        })
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<mixed_behaviour::Event>) {
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
                panic!("Unexpected event {event:?}");
            }
        }
    }

    fn handle_db_executor_result(
        &mut self,
        res: Result<db_executor::QueryId, db_executor::DBExecutorError>,
    ) {
        match res {
            Ok(query_id) => {
                // TODO: in case we want to do bookkeeping, this is the place.
                debug!("Query completed successfully. query_id: {query_id:?}");
            }
            Err(err) => {
                if err.should_log_in_error_level() {
                    error!("Query failed. error: {err:?}");
                } else {
                    debug!("Query failed. error: {err:?}");
                }
            }
        };
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
            mixed_behaviour::ExternalEvent::StreamedBytes(event) => {
                self.handle_stream_bytes_behaviour_event(event);
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
        self.swarm.behaviour_mut().streamed_bytes.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().peer_manager.on_other_behaviour_event(&event);
        self.swarm.behaviour_mut().gossipsub.on_other_behaviour_event(&event);
    }

    fn handle_stream_bytes_behaviour_event(
        &mut self,
        event: streamed_bytes::behaviour::ExternalEvent,
    ) {
        match event {
            streamed_bytes::behaviour::ExternalEvent::NewInboundSession {
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
                let (sender, receiver) = futures::channel::mpsc::channel(self.header_buffer_size);
                // TODO: use query id for bookkeeping.
                // TODO: consider returning error instead of panic.
                let protocol =
                    Protocol::try_from(protocol_name).expect("Encountered unknown protocol");
                let internal_query = protocol.bytes_query_to_protobuf_request(query);
                let data_type = DataType::from(protocol);
                let query_id = self.db_executor.register_query(internal_query, data_type, sender);
                self.query_id_to_inbound_session_id.insert(query_id, inbound_session_id);
                self.query_results_router.push(
                    receiver
                        .chain(stream::once(async move { Data::Fin(data_type) }))
                        .map(move |data| (data, inbound_session_id))
                        .boxed(),
                );
            }
            streamed_bytes::behaviour::ExternalEvent::ReceivedData {
                outbound_session_id,
                data,
            } => {
                trace!(
                    "Received data from peer for session id: {outbound_session_id:?}. sending to \
                     sync subscriber."
                );
                if let Some((_, response_senders)) = self.sync_subscriber_channels.as_mut() {
                    let protocol = self
                        .outbound_session_id_to_protocol
                        .get(&outbound_session_id)
                        .expect("Received data from an unknown session id");
                    match response_senders.try_send(*protocol, data) {
                        Err(RouterError::NoSenderForProtocol { protocol }) => {
                            error!(
                                "The response sender does't support protocol: {protocol:?}. \
                                 Dropping data. outbound_session_id: {outbound_session_id:?}"
                            );
                        }
                        Err(RouterError::TrySendError(e)) => {
                            if e.is_disconnected() {
                                panic!("Receiver was dropped. This should never happen.")
                            } else if e.is_full() {
                                error!(
                                    "Receiver buffer is full. Dropping data. outbound_session_id: \
                                     {outbound_session_id:?}"
                                );
                            }
                        }
                        Ok(()) => {}
                    }
                }
            }
            streamed_bytes::behaviour::ExternalEvent::SessionFailed { session_id, error } => {
                error!("Session {session_id:?} failed on {error:?}");
                self.report_session_removed_to_metrics(session_id);
                // TODO: Handle reputation and retry.
                if let SessionId::OutboundSessionId(outbound_session_id) = session_id {
                    self.outbound_session_id_to_protocol.remove(&outbound_session_id);
                }
            }
            streamed_bytes::behaviour::ExternalEvent::SessionFinishedSuccessfully {
                session_id,
            } => {
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
                let Some(sender) = self.broadcasted_messages_senders.get_mut(&topic_hash) else {
                    error!(
                        "Received a message from a topic we're not subscribed to with hash \
                         {topic_hash:?}"
                    );
                    return;
                };
                let reported_peer_sender = self.reported_peer_sender.clone();
                let send_result = sender.try_send((
                    message,
                    Box::new(move || {
                        // TODO(shahak): Check if we can panic in case of error.
                        let _ = reported_peer_sender.unbounded_send(originated_peer_id);
                    }),
                ));
                if let Err(e) = send_result {
                    if e.is_disconnected() {
                        panic!("Receiver was dropped. This should never happen.")
                    } else if e.is_full() {
                        error!(
                            "Receiver buffer is full. Dropping broadcasted message for topic with \
                             hash {topic_hash:?}."
                        );
                    }
                }
            }
        }
    }

    fn handle_query_result_routing_to_other_peer(&mut self, res: (Data, InboundSessionId)) {
        if self.query_results_router.is_empty() {
            // We're done handling all the queries we had and the stream is exhausted.
            // Creating a new stream collection to process new queries.
            self.query_results_router = StreamCollection::new();
        }
        let (data, inbound_session_id) = res;
        let is_fin = matches!(data, Data::Fin(_));
        let mut data_bytes = vec![];
        data.encode_with_length_prefix(&mut data_bytes).expect("failed to encode data");
        self.swarm.send_length_prefixed_data(data_bytes, inbound_session_id).unwrap_or_else(|e| {
            error!(
                "Failed to send data to peer. Session id: {inbound_session_id:?} not found error: \
                 {e:?}"
            );
        });
        if is_fin {
            self.swarm.close_inbound_session(inbound_session_id).unwrap_or_else(|e| {
                error!(
                    "Failed to close session after Fin. Session id: {inbound_session_id:?} not \
                     found error: {e:?}"
                )
            });
        }
    }

    fn handle_sync_subscriber_query(&mut self, query: Query) {
        let data_type = query.data_type;
        let protocol = data_type.into();
        let mut query_bytes = vec![];
        query.encode(&mut query_bytes).expect("failed to encode query");
        match self.swarm.send_query(query_bytes, PeerId::random(), protocol) {
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
}

pub type NetworkManager =
    GenericNetworkManager<BlockHeaderDBExecutor, Swarm<mixed_behaviour::MixedBehaviour>>;

impl NetworkManager {
    pub fn new(config: NetworkConfig, storage_reader: StorageReader) -> Self {
        let NetworkConfig {
            tcp_port,
            quic_port: _,
            session_timeout,
            idle_connection_timeout,
            header_buffer_size,
            bootstrap_peer_multiaddr,
        } = config;

        let listen_addresses = vec![
            // TODO: uncomment once quic transpot works.
            // format!("/ip4/0.0.0.0/udp/{quic_port}/quic-v1"),
            format!("/ip4/0.0.0.0/tcp/{tcp_port}"),
        ];
        let swarm = build_swarm(listen_addresses, idle_connection_timeout, |key| {
            mixed_behaviour::MixedBehaviour::new(
                key,
                bootstrap_peer_multiaddr.clone(),
                streamed_bytes::Config {
                    session_timeout,
                    supported_inbound_protocols: vec![
                        Protocol::SignedBlockHeader.into(),
                        Protocol::StateDiff.into(),
                    ],
                },
            )
        });

        let db_executor = BlockHeaderDBExecutor::new(storage_reader);
        Self::generic_new(swarm, db_executor, header_buffer_size)
    }

    pub fn get_own_peer_id(&self) -> String {
        self.swarm.local_peer_id().to_string()
    }
}

// TODO(shahak): Change to a wrapper of PeerId if Box dyn becomes an overhead.
pub type ReportCallback = Box<dyn Fn() + Send>;

// TODO(shahak): Make this generic in a type that implements TryFrom<Bytes> and Into<Bytes>.
pub struct BroadcastSubscriberChannels<T, E> {
    pub messages_to_broadcast_sender: With<
        Sender<Bytes>,
        Bytes,
        T,
        Ready<Result<Bytes, SendError>>,
        fn(T) -> Ready<Result<Bytes, SendError>>,
    >,
    pub broadcasted_messages_receiver: Map<
        Receiver<(Bytes, ReportCallback)>,
        fn((Bytes, ReportCallback)) -> (Result<T, E>, ReportCallback),
    >,
}
