mod swarm_trait;

#[cfg(test)]
mod test;

use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, Sender};
use futures::future::pending;
use futures::stream::{BoxStream, SelectAll};
use futures::{FutureExt, StreamExt};
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm};
use papyrus_storage::StorageReader;
use prost::Message;
use tracing::{debug, error, trace};

use self::swarm_trait::SwarmTrait;
use crate::bin_utils::{build_swarm, dial};
use crate::converters::Router;
use crate::db_executor::{self, BlockHeaderDBExecutor, DBExecutor, Data, QueryId};
use crate::protobuf_messages::protobuf;
use crate::streamed_bytes::behaviour::{Behaviour, SessionError};
use crate::streamed_bytes::{Config, Event, InboundSessionId};
use crate::{NetworkConfig, Protocol, Query, ResponseReceivers};

type StreamCollection = SelectAll<BoxStream<'static, (Data, InboundSessionId)>>;
type SyncSubscriberChannels = (Receiver<Query>, Router);

pub struct GenericNetworkManager<DBExecutorT: DBExecutor, SwarmT: SwarmTrait> {
    swarm: SwarmT,
    db_executor: DBExecutorT,
    header_buffer_size: usize,
    query_results_router: StreamCollection,
    sync_subscriber_channels: Option<SyncSubscriberChannels>,
    query_id_to_inbound_session_id: HashMap<QueryId, InboundSessionId>,
}

impl<DBExecutorT: DBExecutor, SwarmT: SwarmTrait> GenericNetworkManager<DBExecutorT, SwarmT> {
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.db_executor.next() => self.handle_db_executor_result(res),
                Some(res) = self.query_results_router.next() => self.handle_query_result_routing(res),
                Some(res) = self.sync_subscriber_channels.as_mut()
                .map(|(query_receiver, _)| query_receiver.next().boxed())
                .unwrap_or(pending().boxed()) => self.handle_sync_subscriber_query(res),
            }
        }
    }

    pub(self) fn generic_new(
        swarm: SwarmT,
        db_executor: DBExecutorT,
        header_buffer_size: usize,
    ) -> Self {
        Self {
            swarm,
            db_executor,
            header_buffer_size,
            query_results_router: StreamCollection::new(),
            sync_subscriber_channels: None,
            query_id_to_inbound_session_id: HashMap::new(),
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

    fn handle_swarm_event(&mut self, event: SwarmEvent<Event<SessionError>>) {
        match event {
            SwarmEvent::ConnectionEstablished { .. } => {
                debug!("Connected to a peer!");
            }
            SwarmEvent::NewListenAddr { .. }
            | SwarmEvent::IncomingConnection { .. }
            | SwarmEvent::ConnectionClosed { .. } => {}
            SwarmEvent::Behaviour(event) => {
                self.handle_behaviour_event(event);
            }
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
                if let Some(query_id) = err.query_id() {
                    // TODO: Consider retrying based on error.
                    let Some(inbound_session_id) =
                        self.query_id_to_inbound_session_id.remove(&query_id)
                    else {
                        error!("Received error on non existing query");
                        return;
                    };
                    // TODO: consider moving conversion to db executor side so network manager would only know bytes.
                    let mut fin_bytes = vec![];
                    <Data as Into<protobuf::BlockHeadersResponse>>::into(Data::Fin)
                        .encode(&mut fin_bytes)
                        .expect("faild to convert fin to bytes");
                    if self.swarm.send_data(fin_bytes, inbound_session_id).is_err() {
                        error!(
                            "Tried to close inbound session {inbound_session_id:?} due to {err:?} \
                             but the session was already closed"
                        );
                    }
                }
            }
        };
    }

    fn handle_behaviour_event(&mut self, event: Event<SessionError>) {
        match event {
            Event::NewInboundSession {
                query,
                inbound_session_id,
                peer_id: _,
                protocol_name: _,
            } => {
                trace!(
                    "Received new inbound query: {query:?} for session id: {inbound_session_id:?}"
                );
                let (sender, receiver) = futures::channel::mpsc::channel(self.header_buffer_size);
                // TODO: use query id for bookkeeping.
                // TODOL consider moving conversion out of network manager.
                let internal_query = protobuf::BlockHeadersRequest::decode(&query[..])
                    .expect("failed to decode protobuf BlockHeadersRequest")
                    .try_into()
                    .expect("failed to convert BlockHeadersRequest");
                let query_id = self.db_executor.register_query(internal_query, sender);
                self.query_id_to_inbound_session_id.insert(query_id, inbound_session_id);
                self.query_results_router
                    .push(receiver.map(move |data| (data, inbound_session_id)).boxed());
            }
            Event::ReceivedData { outbound_session_id, data } => {
                debug!(
                    "Received data from peer for session id: {outbound_session_id:?}. sending to \
                     sync subscriber."
                );
                if let Some((_, response_senders)) = self.sync_subscriber_channels.as_mut() {
                    // TODO: once we have more protocols map session id to protocol.
                    if let Err(e) = response_senders.try_send(Protocol::SignedBlockHeader, data) {
                        error!("Failed to send data to sync subscriber. error: {e:?}");
                    }
                }
            }
            Event::SessionFailed { session_id, error } => {
                debug!("Session {session_id:?} failed on {error:?}");
                // TODO: Handle reputation and retry.
            }
            Event::SessionFinishedSuccessfully { session_id } => {
                debug!("Session completed successfully. session_id: {session_id:?}");
            }
        }
    }

    fn handle_query_result_routing(&mut self, res: (Data, InboundSessionId)) {
        if self.query_results_router.is_empty() {
            // We're done handling all the queries we had and the stream is exhausted.
            // Creating a new stream collection to process new queries.
            self.query_results_router = StreamCollection::new();
        }
        let (data, inbound_session_id) = res;
        let mut data_bytes = vec![];
        <Data as Into<protobuf::BlockHeadersResponse>>::into(data)
            .encode(&mut data_bytes)
            .expect("failed to convert data to bytes");
        self.swarm.send_data(data_bytes, inbound_session_id).unwrap_or_else(|e| {
            error!("Failed to send data to peer. Session id not found error: {e:?}");
        })
    }

    fn handle_sync_subscriber_query(&mut self, query: Query) {
        // TODO: get peer id somehow.
        let peer_id = PeerId::random();
        let mut query_bytes = vec![];
        <Query as Into<protobuf::BlockHeadersRequest>>::into(query)
            .encode(&mut query_bytes)
            .expect("failed to convert query to bytes");
        match self.swarm.send_query(query_bytes, peer_id, Protocol::SignedBlockHeader) {
            Ok(outbound_session_id) => {
                debug!(
                    "Sent query to peer. peer_id: {peer_id:?}, \
                     outbound_session_id: {outbound_session_id:?}"
                );
            }
            Err(e) => error!("Failed to send query to peer. Peer not connected error: {e:?}"),
        }
    }
}

pub type NetworkManager = GenericNetworkManager<BlockHeaderDBExecutor, Swarm<Behaviour>>;

impl NetworkManager {
    // TODO: add tests for this struct.
    // TODO: make sure errors are handled and not just paniced.
    pub fn new(config: NetworkConfig, storage_reader: StorageReader) -> Self {
        let NetworkConfig {
            tcp_port,
            quic_port,
            session_timeout,
            idle_connection_timeout,
            header_buffer_size,
        } = config;

        let listen_addresses = vec![
            format!("/ip4/127.0.0.1/udp/{quic_port}/quic-v1"),
            format!("/ip4/127.0.0.1/tcp/{tcp_port}"),
        ];
        let swarm = build_swarm(
            listen_addresses,
            idle_connection_timeout,
            Behaviour::new(Config {
                session_timeout,
                supported_inbound_protocols: vec![Protocol::SignedBlockHeader.into()],
            }),
        );

        let db_executor = BlockHeaderDBExecutor::new(storage_reader);
        Self::generic_new(swarm, db_executor, header_buffer_size)
    }

    // TODO(shahak): Move this to the constructor and add the address to the config once we have
    // p2p sync.
    pub fn dial(&mut self, dial_address: &str) {
        dial(&mut self.swarm, dial_address);
    }
}
