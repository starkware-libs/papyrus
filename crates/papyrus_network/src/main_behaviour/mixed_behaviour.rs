use libp2p::kad::store::MemoryStore;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identify, kad};

use crate::discovery::kad_impl::KadFromOtherBehaviourEvent;
use crate::{discovery, peer_manager, streamed_bytes};

// TODO: consider reducing the pulicity of all behaviour to pub(crate)
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
    pub peer_manager: peer_manager::PeerManager<peer_manager::peer::Peer>,
    pub discovery: discovery::Behaviour,
    pub identify: identify::Behaviour,
    // TODO(shahak): Consider using a different store.
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub streamed_bytes: streamed_bytes::Behaviour,
}

pub enum Event {
    ExternalEvent(ExternalEvent),
    #[allow(dead_code)]
    InternalEvent(InternalEvent),
}

pub enum ExternalEvent {
    StreamedBytes(streamed_bytes::behaviour::ExternalEvent),
}

pub enum InternalEvent {
    NoOp,
    NotifyKad(KadFromOtherBehaviourEvent),
    NotifyDiscovery(discovery::FromOtherBehaviourEvent),
    NotifyPeerManager(peer_manager::FromOtherBehaviour),
    NotifyStreamedBytes(streamed_bytes::behaviour::FromOtherBehaviour),
}

pub trait BridgedBehaviour {
    fn on_other_behaviour_event(&mut self, event: InternalEvent);
}
