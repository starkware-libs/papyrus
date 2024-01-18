use std::collections::HashMap;

use futures::future::{select, Either};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use tracing::debug;

use crate::block_headers::behaviour::Behaviour;
use crate::block_headers::Event;
use crate::db_executor::{self, DBExecutor, Data, QueryId};
use crate::streamed_data::InboundSessionId;

pub struct NetworkManager {
    swarm: Swarm<Behaviour>,
    // TODO: migrate to a real executor once we have one.
    db_executor: db_executor::dummy_executor::DummyDBExecutor,
    query_id_to_inbound_session: HashMap<QueryId, InboundSessionId>,
}

impl NetworkManager {
    // TODO: add tests for this struct.
    pub fn new(swarm: Swarm<Behaviour>) -> Self {
        Self {
            swarm,
            db_executor: db_executor::dummy_executor::DummyDBExecutor::new(),
            query_id_to_inbound_session: HashMap::new(),
        }
    }

    pub async fn run(&mut self) {
        loop {
            match select(self.swarm.next(), self.db_executor.next()).await {
                Either::Left((Some(event), _)) => self.handle_swarm_event(event),
                Either::Right((Some(res), _)) => self.handle_db_executor_result(res),
                Either::Left((None, _)) => {
                    panic!("Swarm stream ended unexpectedly");
                }
                Either::Right((None, _)) => {
                    panic!("DB executor stream ended unexpectedly");
                }
            };
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

    fn handle_db_executor_result(&mut self, _res: (QueryId, Data)) {
        unimplemented!("handle_db_executor_result")
    }

    fn handle_behaviour_event(&mut self, event: Event) {
        match event {
            Event::NewInboundQuery { query, inbound_session_id } => {
                debug!(
                    "Received new inbound query: {query:?} for session id: {inbound_session_id:?}"
                );
                let query_id = self.db_executor.register_query(query);
                self.query_id_to_inbound_session.insert(query_id, inbound_session_id);
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
            Event::SessionCompletedSuccessfully { .. } => {
                // TODO: consider removing this event.
                unimplemented!("SessionCompletedSuccessfully");
            }
        }
    }
}
