#[cfg(test)]
#[path = "behaviour_test.rs"]
mod behaviour_test;

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

use super::handler::{Handler, NewRequestEvent};
use super::RequestId;
use crate::messages::block::GetBlocks;

#[derive(Debug)]
pub enum Event {
    // TODO(shahak): Implement.
}

pub struct Behaviour {
    substream_timeout: Duration,
    pending_events: VecDeque<ToSwarm<Event, NewRequestEvent>>,
    pending_requests: DefaultHashMap<PeerId, Vec<(GetBlocks, RequestId)>>,
    connected_peers: HashSet<PeerId>,
    next_request_id: RequestId,
}

impl Behaviour {
    pub fn new(substream_timeout: Duration) -> Self {
        Self {
            substream_timeout,
            pending_events: Default::default(),
            pending_requests: Default::default(),
            connected_peers: Default::default(),
            next_request_id: Default::default(),
        }
    }

    pub fn send_request(&mut self, request: GetBlocks, peer_id: PeerId) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id.0 += 1;
        if self.connected_peers.contains(&peer_id) {
            self.send_request_to_handler(peer_id, request.clone(), request_id);
            return request_id;
        }
        self.pending_events.push_back(ToSwarm::Dial {
            opts: DialOpts::peer_id(peer_id).condition(PeerCondition::Disconnected).build(),
        });
        self.pending_requests.get_mut(peer_id).push((request, request_id));
        request_id
    }

    fn send_request_to_handler(
        &mut self,
        peer_id: PeerId,
        request: GetBlocks,
        request_id: RequestId,
    ) {
        self.pending_events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::Any,
            event: NewRequestEvent { request, request_id },
        });
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler::new(self.substream_timeout))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(Handler::new(self.substream_timeout))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_, Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                let ConnectionEstablished { peer_id, .. } = connection_established;
                if let Some(requests) = self.pending_requests.remove(&peer_id) {
                    for (request, request_id) in requests.into_iter() {
                        self.send_request_to_handler(peer_id, request, request_id);
                    }
                }
            }
            _ => {
                // TODO(shahak): Implement.
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
