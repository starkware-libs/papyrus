use libp2p::identify;

use super::kad_impl::KadFromOtherBehaviourEvent;
use crate::main_behaviour::mixed_behaviour;

impl From<identify::Event> for mixed_behaviour::Event {
    fn from(event: identify::Event) -> Self {
        match event {
            identify::Event::Received { peer_id, info } => {
                mixed_behaviour::Event::InternalEvent(mixed_behaviour::InternalEvent::NotifyKad(
                    KadFromOtherBehaviourEvent::FoundListenAddresses {
                        peer_id,
                        listen_addresses: info.listen_addrs,
                    },
                ))
            }
            // TODO(shahak): Consider logging error events.
            _ => mixed_behaviour::Event::InternalEvent(mixed_behaviour::InternalEvent::NoOp),
        }
    }
}
