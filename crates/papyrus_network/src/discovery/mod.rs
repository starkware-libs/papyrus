#[cfg(test)]
mod discovery_test;
#[cfg(test)]
mod flow_test;
pub mod identify_impl;
pub mod kad_impl;

use std::task::{ready, Context, Poll, Waker};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::{pin_mut, Future, FutureExt};
use kad_impl::KadToOtherBehaviourEvent;
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

use crate::mixed_behaviour::BridgedBehaviour;
use crate::{mixed_behaviour, peer_manager};

// TODO(shahak): Consider adding to config.
const DIAL_SLEEP: Duration = Duration::from_secs(5);

pub struct Behaviour {
    is_paused: bool,
    // TODO(shahak): Consider running several queries in parallel
    is_query_running: bool,
    bootstrap_peer_address: Multiaddr,
    bootstrap_peer_id: PeerId,
    is_dialing_to_bootstrap_peer: bool,
    // This needs to be boxed to allow polling it from a &mut.
    sleep_future_for_dialing_bootstrap_peer: Option<BoxFuture<'static, ()>>,
    is_connected_to_bootstrap_peer: bool,
    is_bootstrap_in_kad_routing_table: bool,
    wakers: Vec<Waker>,
}

#[derive(Debug)]
pub enum ToOtherBehaviourEvent {
    RequestKadQuery(PeerId),
    FoundListenAddresses { peer_id: PeerId, listen_addresses: Vec<Multiaddr> },
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

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
                // For the case that the reason for failure is consistent (e.g the bootstrap peer
                // is down), we sleep before redialing
                // TODO(shahak): Consider increasing the time after each failure, the same way we
                // do in starknet client.
                self.sleep_future_for_dialing_bootstrap_peer =
                    Some(tokio::time::sleep(DIAL_SLEEP).boxed());
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
            if let Some(sleep_future) = &mut self.sleep_future_for_dialing_bootstrap_peer {
                pin_mut!(sleep_future);
                ready!(sleep_future.poll(cx));
            }
            self.is_dialing_to_bootstrap_peer = true;
            self.sleep_future_for_dialing_bootstrap_peer = None;
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
        if !self.is_bootstrap_in_kad_routing_table {
            self.is_bootstrap_in_kad_routing_table = true;
            return Poll::Ready(ToSwarm::GenerateEvent(
                ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id: self.bootstrap_peer_id,
                    listen_addresses: vec![self.bootstrap_peer_address.clone()],
                },
            ));
        }

        if !self.is_paused && !self.is_query_running {
            self.is_query_running = true;
            Poll::Ready(ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(
                libp2p::identity::PeerId::random(),
            )))
        } else {
            self.wakers.push(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Behaviour {
    // TODO(shahak): Add support to discovery from multiple bootstrap nodes.
    // TODO(shahak): Add support to multiple addresses for bootstrap node.
    pub fn new(bootstrap_peer_id: PeerId, bootstrap_peer_address: Multiaddr) -> Self {
        Self {
            is_paused: false,
            is_query_running: false,
            bootstrap_peer_id,
            bootstrap_peer_address,
            is_dialing_to_bootstrap_peer: false,
            sleep_future_for_dialing_bootstrap_peer: None,
            is_connected_to_bootstrap_peer: false,
            is_bootstrap_in_kad_routing_table: false,
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

impl From<ToOtherBehaviourEvent> for mixed_behaviour::Event {
    fn from(event: ToOtherBehaviourEvent) -> Self {
        mixed_behaviour::Event::ToOtherBehaviourEvent(
            mixed_behaviour::ToOtherBehaviourEvent::Discovery(event),
        )
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, event: &mixed_behaviour::ToOtherBehaviourEvent) {
        match event {
            mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
                peer_manager::ToOtherBehaviourEvent::PauseDiscovery,
            ) => self.is_paused = true,
            mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
                peer_manager::ToOtherBehaviourEvent::ResumeDiscovery,
            ) => {
                for waker in self.wakers.drain(..) {
                    waker.wake();
                }
                self.is_paused = false;
            }
            mixed_behaviour::ToOtherBehaviourEvent::Kad(
                KadToOtherBehaviourEvent::KadQueryFinished,
            ) => {
                for waker in self.wakers.drain(..) {
                    waker.wake();
                }
                self.is_query_running = false;
            }
            _ => {}
        }
    }
}
