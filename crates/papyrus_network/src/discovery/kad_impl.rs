use libp2p::{kad, Multiaddr, PeerId};
use tracing::error;

use crate::discovery;
use crate::main_behaviour::mixed_behaviour;
use crate::main_behaviour::mixed_behaviour::BridgedBehaviour;

#[derive(Debug)]
pub enum KadFromOtherBehaviourEvent {
    RequestKadQuery(PeerId),
    FoundListenAddresses { peer_id: PeerId, listen_addresses: Vec<Multiaddr> },
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
                mixed_behaviour::Event::InternalEvent(
                    mixed_behaviour::InternalEvent::NotifyDiscovery(
                        discovery::FromOtherBehaviourEvent::KadQueryFinished,
                    ),
                )
            }
            _ => mixed_behaviour::Event::InternalEvent(mixed_behaviour::InternalEvent::NoOp),
        }
    }
}

impl<TStore: kad::store::RecordStore + Send + 'static> BridgedBehaviour for kad::Behaviour<TStore> {
    fn on_other_behaviour_event(&mut self, event: mixed_behaviour::InternalEvent) {
        let mixed_behaviour::InternalEvent::NotifyKad(event) = event else {
            return;
        };
        match event {
            KadFromOtherBehaviourEvent::RequestKadQuery(peer_id) => {
                self.get_closest_peers(peer_id);
            }
            KadFromOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses } => {
                for address in listen_addresses {
                    self.add_address(&peer_id, address);
                }
            }
        }
    }
}
