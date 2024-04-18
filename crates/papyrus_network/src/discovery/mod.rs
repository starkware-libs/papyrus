#[cfg(test)]
mod discovery_test;
pub mod identify_impl;
pub mod kad_impl;

use std::task::{Context, Poll, Waker};

use kad_impl::KadFromOtherBehaviourEvent;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{
    dummy,
    AddressChange,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    DialFailure,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use crate::main_behaviour::mixed_behaviour;
use crate::main_behaviour::mixed_behaviour::BridgedBehaviour;

pub struct Behaviour {
    is_paused: bool,
    // TODO(shahak): Consider running several queries in parallel
    is_query_running: bool,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    is_connected_to_bootstrap_peer: bool,
    wakers: Vec<Waker>,
}

#[derive(Debug)]
pub enum FromOtherBehaviourEvent {
    KadQueryFinished,
    PauseDiscovery,
    #[allow(dead_code)]
    ResumeDiscovery,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct RequestKadQuery(PeerId);

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = RequestKadQuery;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_dialing_to_bootstrap_peer = false;
            }
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                self.is_connected_to_bootstrap_peer = true;
                self.is_dialing_to_bootstrap_peer = false;
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) if peer_id == self.bootstrap_peer_id && remaining_established == 0 => {
                self.is_connected_to_bootstrap_peer = false;
                self.is_dialing_to_bootstrap_peer = false;
            }
            FromSwarm::AddressChange(AddressChange { peer_id, .. })
                if peer_id == self.bootstrap_peer_id =>
            {
                todo!();
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        if !self.is_dialing_to_bootstrap_peer && !self.is_connected_to_bootstrap_peer {
            self.is_dialing_to_bootstrap_peer = true;
            return Poll::Ready(ToSwarm::Dial {
                opts: DialOpts::peer_id(self.bootstrap_peer_id)
                    .addresses(vec![self.bootstrap_peer_address.clone()])
                    // The peer manager might also be dialing to the bootstrap node.
                    .condition(PeerCondition::DisconnectedAndNotDialing)
                    .build(),
            });
        }

        // If we're not connected to any node, then each Kademlia query we make will automatically
        // return without any peers. Running queries in that mode will add unnecessary overload to
        // the swarm.
        if !self.is_connected_to_bootstrap_peer {
            return Poll::Pending;
        }

        if !self.is_paused && !self.is_query_running {
            self.is_query_running = true;
            Poll::Ready(ToSwarm::GenerateEvent(RequestKadQuery(PeerId::random())))
        } else {
            self.wakers.push(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Behaviour {
    #[allow(dead_code)]
    // TODO(shahak): Add support to discovery from multiple bootstrap nodes.
    // TODO(shahak): Add support to multiple addresses for bootstrap node.
    pub fn new(bootstrap_peer_id: PeerId, bootstrap_peer_address: Multiaddr) -> Self {
        Self {
            is_paused: false,
            is_query_running: false,
            bootstrap_peer_id,
            bootstrap_peer_address,
            is_dialing_to_bootstrap_peer: false,
            is_connected_to_bootstrap_peer: false,
            wakers: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn bootstrap_peer_id(&self) -> PeerId {
        self.bootstrap_peer_id
    }

    #[cfg(test)]
    pub fn bootstrap_peer_address(&self) -> &Multiaddr {
        &self.bootstrap_peer_address
    }
}

impl From<RequestKadQuery> for mixed_behaviour::Event {
    fn from(event: RequestKadQuery) -> Self {
        mixed_behaviour::Event::InternalEvent(mixed_behaviour::InternalEvent::NotifyKad(
            KadFromOtherBehaviourEvent::RequestKadQuery(event.0),
        ))
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, event: mixed_behaviour::InternalEvent) {
        let mixed_behaviour::InternalEvent::NotifyDiscovery(event) = event else {
            return;
        };
        match event {
            FromOtherBehaviourEvent::PauseDiscovery => self.is_paused = true,
            FromOtherBehaviourEvent::ResumeDiscovery => {
                for waker in self.wakers.drain(..) {
                    waker.wake();
                }
                self.is_paused = false;
            }
            FromOtherBehaviourEvent::KadQueryFinished => {
                for waker in self.wakers.drain(..) {
                    waker.wake();
                }
                self.is_query_running = false;
            }
        }
    }
}
