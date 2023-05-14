use libp2p::identify;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaEvent};
use libp2p_swarm_derive::NetworkBehaviour;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MixedEvent")]
pub struct MixedBehaviour {
    pub kademlia: Kademlia<MemoryStore>,
    pub identify: identify::Behaviour,
}

#[derive(Debug)]
pub enum MixedEvent {
    Kademlia(KademliaEvent),
    Identify(identify::Event),
}

impl From<KademliaEvent> for MixedEvent {
    fn from(event: KademliaEvent) -> Self {
        MixedEvent::Kademlia(event)
    }
}

impl From<identify::Event> for MixedEvent {
    fn from(event: identify::Event) -> Self {
        MixedEvent::Identify(event)
    }
}
