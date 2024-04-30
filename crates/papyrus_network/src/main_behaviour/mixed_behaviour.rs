// TODO(shahak): Erase main_behaviour and make this a separate module.

use libp2p::core::multiaddr::Protocol;
use libp2p::identity::PublicKey;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identify, kad, Multiaddr, PeerId};

use crate::discovery::kad_impl::KadFromOtherBehaviourEvent;
use crate::peer_manager::PeerManagerConfig;
use crate::{discovery, peer_manager, streamed_bytes};

// TODO: consider reducing the pulicity of all behaviour to pub(crate)
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
    pub peer_manager: peer_manager::PeerManager<peer_manager::peer::Peer>,
    pub discovery: Toggle<discovery::Behaviour>,
    pub identify: identify::Behaviour,
    // TODO(shahak): Consider using a different store.
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub streamed_bytes: streamed_bytes::Behaviour,
}

#[derive(Debug)]
pub enum Event {
    ExternalEvent(ExternalEvent),
    InternalEvent(InternalEvent),
}

#[derive(Debug)]
pub enum ExternalEvent {
    StreamedBytes(streamed_bytes::behaviour::ExternalEvent),
}

#[derive(Debug)]
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

impl MixedBehaviour {
    // TODO: get config details from network manager config
    /// Panics if bootstrap_peer_multiaddr doesn't have a peer id.
    pub fn new(
        key: PublicKey,
        bootstrap_peer_multiaddr: Option<Multiaddr>,
        streamed_bytes_config: streamed_bytes::Config,
    ) -> Self {
        let local_peer_id = PeerId::from_public_key(&key);
        Self {
            peer_manager: peer_manager::PeerManager::new(PeerManagerConfig::default()),
            discovery: bootstrap_peer_multiaddr
                .as_ref()
                .map(|bootstrap_peer_multiaddr| {
                    discovery::Behaviour::new(
                        get_peer_id_from_multiaddr(bootstrap_peer_multiaddr)
                            .expect("bootstrap_peer_multiaddr doesn't have a peer id"),
                        bootstrap_peer_multiaddr.clone(),
                    )
                })
                .into(),
            identify: identify::Behaviour::new(identify::Config::new(
                "/staknet/identify/0.1.0-rc.0".to_string(),
                key,
            )),
            // TODO: change kademlia protocol name
            kademlia: kad::Behaviour::new(local_peer_id, MemoryStore::new(local_peer_id)),
            streamed_bytes: streamed_bytes::Behaviour::new(streamed_bytes_config),
        }
    }
}

// TODO(shahak): Open a github issue in libp2p to add this functionality.
fn get_peer_id_from_multiaddr(address: &Multiaddr) -> Option<PeerId> {
    for protocol in address.iter() {
        if let Protocol::P2p(peer_id) = protocol {
            return Some(peer_id);
        }
    }
    None
}
