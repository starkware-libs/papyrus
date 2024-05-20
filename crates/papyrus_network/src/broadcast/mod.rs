use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::swarm::{
    dummy,
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;
// TODO(shahak): move Bytes to a more generic file.
use crate::streamed_bytes::Bytes;

pub struct Behaviour;

pub type Topic = String;

#[derive(Debug)]
pub enum ExternalEvent {
    #[allow(dead_code)]
    Received { originated_peer_id: PeerId, message: Bytes, topic: Topic },
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ExternalEvent;

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
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        // TODO(shahak): Implement this.
        Poll::Pending
    }
}

impl Behaviour {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    #[allow(dead_code)]
    pub fn subscribe_to_topic(&mut self, _topic: Topic) {
        unimplemented!()
    }

    #[allow(dead_code)]
    pub fn broadcast_message(&mut self, _message: Bytes, _topic: Topic) {
        unimplemented!()
    }
}

impl From<ExternalEvent> for mixed_behaviour::Event {
    fn from(event: ExternalEvent) -> Self {
        mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::Broadcast(event))
    }
}

impl BridgedBehaviour for Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {
        // TODO(shahak): Implement this.
    }
}
