use libp2p::kad::store::MemoryStore;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identify, kad};

use crate::discovery::identify_impl::IdentifyInputEvent;
use crate::discovery::kad_impl::KadInputEvent;
use crate::{discovery, streamed_bytes};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
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
    StreamedBytes(streamed_bytes::behaviour::Event),
}

pub enum InternalEvent {
    NoOp,
    NotifyIdentify(IdentifyInputEvent),
    NotifyKad(KadInputEvent),
    NotifyDiscovery(discovery::InputEvent),
}

pub trait BridgedBehaviour {
    fn on_other_behaviour_event(&mut self, event: InternalEvent);
}

impl From<streamed_bytes::behaviour::Event> for Event {
    fn from(event: streamed_bytes::behaviour::Event) -> Self {
        Self::ExternalEvent(ExternalEvent::StreamedBytes(event))
    }
}
