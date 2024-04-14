use libp2p::identify;

use super::kad_impl::KadInputEvent;
use crate::main_behaviour::mixed_behaviour;
use crate::main_behaviour::mixed_behaviour::BridgedBehaviour;

pub enum IdentifyInputEvent {}

impl From<identify::Event> for mixed_behaviour::Event {
    fn from(event: identify::Event) -> Self {
        match event {
            identify::Event::Received { peer_id, info } => mixed_behaviour::Event::InternalEvent(
                mixed_behaviour::InternalEvent::NotifyKad(KadInputEvent::FoundListenAddresses {
                    peer_id,
                    listen_addresses: info.listen_addrs,
                }),
            ),
            // TODO(shahak): Consider logging error events.
            _ => mixed_behaviour::Event::InternalEvent(mixed_behaviour::InternalEvent::NoOp),
        }
    }
}

impl BridgedBehaviour for identify::Behaviour {
    fn on_other_behaviour_event(&mut self, _event: mixed_behaviour::InternalEvent) {}
}
