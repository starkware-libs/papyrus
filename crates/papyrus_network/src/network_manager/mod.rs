use std::collections::HashMap;

use futures::channel::mpsc::{Receiver, Sender};
use futures::stream::{BoxStream, SelectAll};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use papyrus_storage::StorageReader;
use tracing::{debug, error};

use crate::bin_utils::{build_swarm, dial};
use crate::block_headers::behaviour::Behaviour as BlockHeadersBehaviour;
use crate::block_headers::Event;
use crate::db_executor::{self, DBExecutor, Data, QueryId};
use crate::streamed_data::InboundSessionId;
use crate::{NetworkConfig, Query, ResponseReceivers, ResponseSenders};

#[cfg(test)]
mod test;

type StreamCollection = SelectAll<BoxStream<'static, (Data, InboundSessionId)>>;
type SyncSubscriberChannels = (Receiver<Query>, ResponseSenders);

pub struct NetworkManager {
    swarm: Swarm<BlockHeadersBehaviour>,
    db_executor: db_executor::BlockHeaderDBExecutor,
    header_buffer_size: usize,
    query_results_router: StreamCollection,
    sync_subscriber: Option<SyncSubscriberChannels>,
    query_id_to_inbound_session_id: HashMap<QueryId, InboundSessionId>,
}

impl NetworkManager {
    // TODO: add tests for this struct.
    // TODO: make sure errors are handled and not just paniced.
    pub fn new(config: NetworkConfig, storage_reader: StorageReader) -> Self {
        let NetworkConfig {
            listen_addresses,
            session_timeout,
            idle_connection_timeout,
            header_buffer_size,
        } = config;

        let swarm = build_swarm(
            listen_addresses,
            idle_connection_timeout,
            BlockHeadersBehaviour::new(session_timeout),
        );

        let db_executor = db_executor::BlockHeaderDBExecutor::new(storage_reader);
        Self {
            swarm,
            db_executor,
            header_buffer_size,
            query_results_router: StreamCollection::new(),
            sync_subscriber: None,
            query_id_to_inbound_session_id: HashMap::new(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.db_executor.next() => self.handle_db_executor_result(res),
                Some(res) = self.query_results_router.next() => self.handle_query_result_routing(res),
            }
        }
    }

    // TODO(shahak): Move this to the constructor and add the address to the config once we have
    // p2p sync.
    pub fn dial(&mut self, dial_address: &str) {
        dial(&mut self.swarm, dial_address);
    }

    pub fn register_subscriber(&mut self) -> (Sender<Query>, ResponseReceivers) {
        let (sender, query_receiver) = futures::channel::mpsc::channel(self.header_buffer_size);
        let (response_sender, response_receiver) =
            futures::channel::mpsc::channel(self.header_buffer_size);
        self.sync_subscriber =
            Some((query_receiver, ResponseSenders { signed_headers_sender: response_sender }));
        (sender, ResponseReceivers::new(response_receiver))
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<Event>) {
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
                if let Some(query_id) = err.query_id() {
                    debug!("Query failed. error: {err:?}");
                    // TODO: Consider retrying based on error.
                    let Some(inbound_session_id) =
                        self.query_id_to_inbound_session_id.remove(&query_id)
                    else {
                        error!("Received error on non existing query");
                        return;
                    };
                    if self.swarm.behaviour_mut().send_data(Data::Fin, inbound_session_id).is_err()
                    {
                        error!(
                            "Tried to close inbound session {inbound_session_id:?} due to {err:?} \
                             but the session was already closed"
                        );
                    }
                } else {
                    // Logging this error in an error level since it has no side effects and thus
                    // it's harder to track.
                    error!("Query failed. error: {err:?}");
                }
            }
        };
    }

    fn handle_behaviour_event(&mut self, event: Event) {
        match event {
            Event::NewInboundQuery { query, inbound_session_id } => {
                debug!(
                    "Received new inbound query: {query:?} for session id: {inbound_session_id:?}"
                );
                let (sender, receiver) = futures::channel::mpsc::channel(self.header_buffer_size);
                // TODO: use query id for bookkeeping.
                let query_id = self.db_executor.register_query(query, sender);
                self.query_id_to_inbound_session_id.insert(query_id, inbound_session_id);
                self.query_results_router
                    .push(receiver.map(move |data| (data, inbound_session_id)).boxed());
            }
            Event::ReceivedData { .. } => {
                // TODO: Do something with the received data.
            }
            Event::SessionFailed { session_id, session_error } => {
                debug!("Session {session_id:?} failed on {session_error:?}");
                // TODO: Handle reputation and retry.
            }
            Event::QueryConversionError(error) => {
                debug!("Failed to convert incoming query on {error:?}");
                // TODO: Consider adding peer_id to event and handling reputation.
            }
            Event::SessionCompletedSuccessfully { session_id } => {
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
        self.swarm.behaviour_mut().send_data(data, inbound_session_id).unwrap_or_else(|e| {
            error!("Failed to send data to peer. Session id not found error: {e:?}");
        })
    }
}
