use libp2p::kad::store::MemoryStore;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identify, kad};

use crate::streamed_bytes;

// TODO: consider reducing the pulicity of all behaviour to pub(crate)
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
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
    // TODO: move these to internal when we have discovery
    Kademilia(kad::Event),
    Identify(identify::Event),
}

pub enum InternalEvent {
    #[allow(dead_code)]
    NotifyStreamedBytes(streamed_bytes::behaviour::InternalEvent),
}

pub trait BridgedBehaviour {
    fn on_other_behaviour_event(&mut self, event: InternalEvent);
}

impl From<kad::Event> for Event {
    fn from(event: kad::Event) -> Self {
        Self::ExternalEvent(ExternalEvent::Kademilia(event))
    }
}

impl From<identify::Event> for Event {
    fn from(event: identify::Event) -> Self {
        Self::ExternalEvent(ExternalEvent::Identify(event))
    }
}

impl From<streamed_bytes::behaviour::Event> for Event {
    fn from(event: streamed_bytes::behaviour::Event) -> Self {
        Self::ExternalEvent(ExternalEvent::StreamedBytes(event))
    }
}
