// #[cfg(test)]
// #[path = "behaviour_test.rs"]
// mod behaviour_test;

use std::collections::{HashSet, VecDeque};
use std::task::{Context, Poll};
use std::time::Duration;

use defaultmap::DefaultHashMap;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    NotifyHandler,
    PollParameters,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use super::handler::{Handler, RequestFromBehaviourEvent};
use super::{DataBound, InboundSessionId, OutboundSessionId, QueryBound};

#[derive(Debug)]
// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
pub(crate) enum Event<Query: QueryBound, Data: DataBound> {
    NewInboundQuery { query: Query, inbound_session_id: InboundSessionId },
    RecievedData { data: Data, outbound_session_id: OutboundSessionId },
}

// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
pub(crate) struct Behaviour<Query: QueryBound, Data: DataBound> {
    substream_timeout: Duration,
    pending_events: VecDeque<ToSwarm<Event<Query, Data>, RequestFromBehaviourEvent<Query, Data>>>,
    pending_queries: DefaultHashMap<PeerId, Vec<(Query, OutboundSessionId)>>,
    connected_peers: HashSet<PeerId>,
    next_outbound_session_id: OutboundSessionId,
}

// TODO(shahak) remove allow dead code.
#[allow(dead_code)]
impl<Query: QueryBound, Data: DataBound> Behaviour<Query, Data> {
    pub fn new(substream_timeout: Duration) -> Self {
        Self {
            substream_timeout,
            pending_events: Default::default(),
            pending_queries: Default::default(),
            connected_peers: Default::default(),
            next_outbound_session_id: Default::default(),
        }
    }

    /// Send query to the given peer and start a new outbound session with it. Return the id of the
    /// new session.
    pub fn send_query(&mut self, query: Query, peer_id: PeerId) -> OutboundSessionId {
        let outbound_session_id = self.next_outbound_session_id;
        self.next_outbound_session_id.value += 1;
        if self.connected_peers.contains(&peer_id) {
            self.send_query_to_handler(peer_id, query, outbound_session_id);
            return outbound_session_id;
        }
        self.pending_events.push_back(ToSwarm::Dial {
            opts: DialOpts::peer_id(peer_id).condition(PeerCondition::Disconnected).build(),
        });
        self.pending_queries.get_mut(peer_id).push((query, outbound_session_id));
        outbound_session_id
    }

    /// Send a data message to an open inbound session.
    pub fn send_data(&mut self, _data: Data, _inbound_session_id: InboundSessionId) {
        unimplemented!();
    }

    /// Report to the behaviour that we've finished sending all the required data for a given
    /// inbound session.
    pub fn finish_inbound_session(_inbound_session_id: InboundSessionId) {
        unimplemented!();
    }

    /// Report to the behaviour that we've received all the required data from a given outbound
    /// session.
    pub fn finish_outbound_session(_outbound_session_id: OutboundSessionId) {
        unimplemented!();
    }

    fn send_query_to_handler(
        &mut self,
        peer_id: PeerId,
        query: Query,
        outbound_session_id: OutboundSessionId,
    ) {
        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::Any,
            event: RequestFromBehaviourEvent::CreateOutboundSession { query, outbound_session_id },
        });
    }
}

impl<Query: QueryBound, Data: DataBound> NetworkBehaviour for Behaviour<Query, Data> {
    type ConnectionHandler = Handler<Query, Data>;
    type ToSwarm = Event<Query, Data>;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        // Ok(Handler::new(self.substream_timeout))
        unimplemented!();
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        // Ok(Handler::new(self.substream_timeout))
        unimplemented!();
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_, Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                let ConnectionEstablished { peer_id, .. } = connection_established;
                if let Some(queries) = self.pending_queries.remove(&peer_id) {
                    for (query, outbound_session_id) in queries.into_iter() {
                        self.send_query_to_handler(peer_id, query, outbound_session_id);
                    }
                }
            }
            _ => {
                unimplemented!();
            }
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        // TODO(shahak): Implement.
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
        _params: &mut impl PollParameters,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }
        Poll::Pending
    }
}
