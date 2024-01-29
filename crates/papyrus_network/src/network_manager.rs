use futures::stream::{BoxStream, SelectAll};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use papyrus_storage::StorageReader;
use tracing::debug;

use crate::block_headers::behaviour::Behaviour;
use crate::block_headers::Event;
use crate::db_executor::{self, DBExecutor, Data};
use crate::streamed_data::InboundSessionId;

type StreamCollection = SelectAll<BoxStream<'static, (Data, InboundSessionId)>>;

pub struct NetworkManager {
    swarm: Swarm<Behaviour>,
    db_executor: db_executor::BlockHeaderDBExecutor,
    buffer_size: usize,
    query_results_router: StreamCollection,
}

impl<'a> NetworkManager {
    // TODO: add tests for this struct.
    // TODO: make sure errors are handled and not just paniced.
    pub fn new(swarm: Swarm<Behaviour>, storage_reader: StorageReader) -> Self {
        let db_executor = db_executor::BlockHeaderDBExecutor::new(storage_reader);
        Self { swarm, db_executor, buffer_size: 1000, query_results_router: SelectAll::new() }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => self.handle_swarm_event(event),
                Some(res) = self.db_executor.next() => self.handle_db_executor_result(res),
                res = self.query_results_router.next() => self.handle_query_results_routing(res),
            }
        }
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
                let _query_id = self.db_executor.register_query(query, sender);
                self.query_results_router
                    .push(receiver.map(move |data| (data, inbound_session_id)).boxed());
                // TODO: use query id for bookkeeping.
                // self.query_id_to_inbound_session.insert(query_id, (inbound_session_id,
                // receiver));
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

    fn handle_query_results_routing(&mut self, res: Option<(Data, InboundSessionId)>) {
        match res {
            None => {
                // we're done handling all the queries we had. need to reset the router.
                self.query_results_router = SelectAll::new();
            }
            Some((data, inbound_session_id)) => {
                if let Err(e) = self.swarm.behaviour_mut().send_data(data, inbound_session_id) {
                    panic!("Failed to send data to peer. Session id not found error: {e:?}")
                }
            }
        }
    }
}
