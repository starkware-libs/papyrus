use libp2p::kad;
use tracing::error;

use super::identify_impl::IdentifyToOtherBehaviourEvent;
use crate::mixed_behaviour;
use crate::mixed_behaviour::BridgedBehaviour;

#[derive(Debug)]
pub enum KadToOtherBehaviourEvent {
    KadQueryFinished,
}

impl From<kad::Event> for mixed_behaviour::Event {
    fn from(event: kad::Event) -> Self {
        match event {
            kad::Event::OutboundQueryProgressed {
                id: _,
                result: kad::QueryResult::GetClosestPeers(result),
                ..
            } => {
                if let Err(err) = result {
                    error!("Kademlia query failed on {err:?}");
                }
                mixed_behaviour::Event::ToOtherBehaviourEvent(
                    mixed_behaviour::ToOtherBehaviourEvent::Kad(
                        KadToOtherBehaviourEvent::KadQueryFinished,
                    ),
                )
            }
            _ => mixed_behaviour::Event::ToOtherBehaviourEvent(
                mixed_behaviour::ToOtherBehaviourEvent::NoOp,
            ),
        }
    }
}

impl<TStore: kad::store::RecordStore + Send + 'static> BridgedBehaviour for kad::Behaviour<TStore> {
    fn on_other_behaviour_event(&mut self, event: &mixed_behaviour::ToOtherBehaviourEvent) {
        match event {
            mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                super::ToOtherBehaviourEvent::RequestKadQuery(peer_id),
            ) => {
                self.get_closest_peers(*peer_id);
            }
            mixed_behaviour::ToOtherBehaviourEvent::Identify(
                IdentifyToOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses },
            )
            | mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                super::ToOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses },
            ) => {
                for address in listen_addresses {
                    self.add_address(peer_id, address.clone());
                }
            }
            _ => {}
        }
    }
}
