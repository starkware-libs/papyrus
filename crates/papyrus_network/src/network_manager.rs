use futures::stream::{BoxStream, SelectAll};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use papyrus_storage::StorageReader;
use tracing::debug;

use crate::bin_utils::{build_swarm, dial};
use crate::block_headers::behaviour::Behaviour as BlockHeadersBehaviour;
use crate::block_headers::Event;
use crate::db_executor::{self, DBExecutor, Data};
use crate::streamed_data::InboundSessionId;
use crate::Config;

// TODO(shahak): Consider adding to config.
const DB_EXECUTOR_BUFFER_SIZE: usize = 100000;

// TODO(shahak): Consider adding to config.
const IDLE_CONNECTION_TIMEOUT_SECONDS: u64 = 10;

type StreamCollection = SelectAll<BoxStream<'static, (Data, InboundSessionId)>>;

pub struct NetworkManager {
    swarm: Swarm<BlockHeadersBehaviour>,
    db_executor: db_executor::BlockHeaderDBExecutor,
    buffer_size: usize,
    query_results_router: StreamCollection,
}

impl NetworkManager {
    // TODO: add tests for this struct.
    // TODO: make sure errors are handled and not just paniced.
    pub fn new(config: Config, storage_reader: StorageReader) -> Self {
        let Config { listen_address, session_timeout } = config;

        let swarm = build_swarm(
            listen_address,
            IDLE_CONNECTION_TIMEOUT_SECONDS,
            BlockHeadersBehaviour::new(session_timeout),
        );

        let db_executor = db_executor::BlockHeaderDBExecutor::new(storage_reader);
        Self {
            swarm,
            db_executor,
            buffer_size: DB_EXECUTOR_BUFFER_SIZE,
            query_results_router: StreamCollection::new(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.db_executor.next() => self.handle_db_executor_result(res),
                res = self.query_results_router.next() => self.handle_query_result_routing(res),
            }
        }
    }

    // TODO(shahak): Move this to the constructor and add the address to the config once we have
    // p2p sync.
    pub fn dial(&mut self, dial_address: &str) {
        dial(&mut self.swarm, dial_address);
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
        &self,
        res: Result<db_executor::QueryId, db_executor::DBExecutorError>,
    ) {
        match res {
            Ok(query_id) => {
                // TODO: in case we want to do bookkeeping, this is the place.
                debug!("Query completed successfully. query_id: {query_id:?}");
            }
            Err(err) => {
                // TODO: how do we handle errors?
                debug!("Query failed. error: {err:?}");
            }
        };
    }

    fn handle_behaviour_event(&mut self, event: Event) {
        match event {
            Event::NewInboundQuery { query, inbound_session_id } => {
                debug!(
                    "Received new inbound query: {query:?} for session id: {inbound_session_id:?}"
                );
                let (sender, receiver) = futures::channel::mpsc::channel(self.buffer_size);
                // TODO: use query id for bookkeeping.
                let _query_id = self.db_executor.register_query(query, sender);
                self.query_results_router
                    .push(receiver.map(move |data| (data, inbound_session_id)).boxed());
            }
            Event::ReceivedData { .. } => {
                unimplemented!("ReceivedData");
            }
            Event::SessionFailed { .. } => {
                unimplemented!("SessionFailed");
            }
            Event::ProtobufConversionError(_) => {
                unimplemented!("ProtobufConversionError");
            }
            Event::SessionCompletedSuccessfully { session_id } => {
                debug!("Session completed successfully. session_id: {session_id:?}");
            }
        }
    }

    fn handle_query_result_routing(&mut self, res: Option<(Data, InboundSessionId)>) {
        match res {
            None => {
                // We're done handling all the queries we had and the stream is exhausted.
                // Creating a new stream collection to process new queries.
                self.query_results_router = StreamCollection::new();
            }
            Some((data, inbound_session_id)) => {
                if let Err(e) = self.swarm.behaviour_mut().send_data(data, inbound_session_id) {
                    panic!("Failed to send data to peer. Session id not found error: {e:?}")
                }
            }
        }
    }
}
