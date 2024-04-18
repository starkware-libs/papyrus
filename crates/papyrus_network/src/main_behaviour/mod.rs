pub(crate) mod mixed_behaviour;

use std::task::{ready, Context, Poll};

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
use mixed_behaviour::MixedBehaviour;

use self::mixed_behaviour::{BridgedBehaviour, Event as MixedBehaviourEvent};

// TODO(shahak): Make this an enum and fill its variants
struct Event;

// TODO(shahak): Find a better name for this.
struct MainBehaviour {
    mixed_behaviour: MixedBehaviour,
}

impl NetworkBehaviour for MainBehaviour {
    type ConnectionHandler = <MixedBehaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event;

    // Required methods
    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.mixed_behaviour.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.mixed_behaviour.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.mixed_behaviour.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        self.mixed_behaviour.on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        let mixed_behaviour_event = ready!(self.mixed_behaviour.poll(cx));
        match mixed_behaviour_event {
            ToSwarm::GenerateEvent(MixedBehaviourEvent::InternalEvent(internal_event)) => {
                match internal_event {
                    mixed_behaviour::InternalEvent::NoOp => {}
                    mixed_behaviour::InternalEvent::NotifyKad(_) => {
                        self.mixed_behaviour.kademlia.on_other_behaviour_event(internal_event)
                    }
                    mixed_behaviour::InternalEvent::NotifyDiscovery(_) => {
                        self.mixed_behaviour.discovery.on_other_behaviour_event(internal_event)
                    }
                    mixed_behaviour::InternalEvent::NotifyStreamedBytes(_) => {
                        self.mixed_behaviour.streamed_bytes.on_other_behaviour_event(internal_event)
                    }
                    mixed_behaviour::InternalEvent::NotifyPeerManager(_) => {
                        self.mixed_behaviour.peer_manager.on_other_behaviour_event(internal_event)
                    }
                }
                Poll::Pending
            }
            _ => Poll::Ready(mixed_behaviour_event.map_out(|_| Event)),
        }
    }
}
