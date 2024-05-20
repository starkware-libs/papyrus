use libp2p::{identify, Multiaddr, PeerId};

use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;

pub const IDENTIFY_PROTOCOL_VERSION: &str = "/staknet/identify/0.1.0-rc.0";

#[derive(Debug)]
pub enum IdentifyToOtherBehaviourEvent {
    FoundListenAddresses { peer_id: PeerId, listen_addresses: Vec<Multiaddr> },
}

impl From<identify::Event> for mixed_behaviour::Event {
    fn from(event: identify::Event) -> Self {
        match event {
            identify::Event::Received { peer_id, info } => {
                mixed_behaviour::Event::ToOtherBehaviourEvent(
                    mixed_behaviour::ToOtherBehaviourEvent::Identify(
                        IdentifyToOtherBehaviourEvent::FoundListenAddresses {
                            peer_id,
                            listen_addresses: info.listen_addrs,
                        },
                    ),
                )
            }
            // TODO(shahak): Consider logging error events.
            _ => mixed_behaviour::Event::ToOtherBehaviourEvent(
                mixed_behaviour::ToOtherBehaviourEvent::NoOp,
            ),
        }
    }
}

impl BridgedBehaviour for identify::Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
