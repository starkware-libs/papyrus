// TODO(shahak): add tests

pub mod identify_impl;
pub mod kad_impl;
mod null_handler;

use std::task::{Context, Poll, Waker};

use kad_impl::KadInputEvent;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use null_handler::NullHandler;

use crate::main_behaviour::mixed_behaviour;
use crate::main_behaviour::mixed_behaviour::BridgedBehaviour;

pub struct Behaviour {
    is_paused: bool,
    is_query_running: bool,
    wakers: Vec<Waker>,
}

pub enum InputEvent {
    KadQueryFinished,
    PauseDiscovery,
    ResumeDiscovery,
}

#[allow(dead_code)]
struct RequestKadQuery(PeerId);

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = NullHandler;
    type ToSwarm = RequestKadQuery;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(NullHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(NullHandler)
    }

    fn on_swarm_event(&mut self, _event: FromSwarm<'_>) {}

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
    pub fn new() -> Self {
        Self { is_paused: false, is_query_running: false, wakers: Vec::new() }
    }
}

impl From<RequestKadQuery> for mixed_behaviour::Event {
    fn from(event: RequestKadQuery) -> Self {
        mixed_behaviour::Event::InternalEvent(mixed_behaviour::InternalEvent::NotifyKad(
            KadInputEvent::RequestKadQuery(event.0),
        ))
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, event: mixed_behaviour::InternalEvent) {
        match event {
            mixed_behaviour::InternalEvent::NotifyDiscovery(InputEvent::PauseDiscovery) => {
                self.is_paused = true
            }
            mixed_behaviour::InternalEvent::NotifyDiscovery(InputEvent::ResumeDiscovery) => {
                self.is_paused = false
            }
            mixed_behaviour::InternalEvent::NotifyDiscovery(InputEvent::KadQueryFinished) => {
                self.is_query_running = false
            }
        }
    }
}
