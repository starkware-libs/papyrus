// TODO(shahak): Erase main_behaviour and make this a separate module.

use libp2p::identity::Keypair;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{gossipsub, identify, kad, Multiaddr, PeerId, StreamProtocol};

use crate::discovery::identify_impl::{IdentifyToOtherBehaviourEvent, IDENTIFY_PROTOCOL_VERSION};
use crate::discovery::kad_impl::KadToOtherBehaviourEvent;
use crate::peer_manager::PeerManagerConfig;
use crate::{discovery, gossipsub_impl, peer_manager, sqmr};

// TODO: consider reducing the pulicity of all behaviour to pub(crate)
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct MixedBehaviour {
    pub peer_manager: peer_manager::PeerManager<peer_manager::peer::Peer>,
    pub discovery: Toggle<discovery::Behaviour>,
    pub identify: identify::Behaviour,
    // TODO(shahak): Consider using a different store.
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub sqmr: sqmr::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
}

#[derive(Debug)]
pub enum Event {
    ExternalEvent(ExternalEvent),
    ToOtherBehaviourEvent(ToOtherBehaviourEvent),
}

#[derive(Debug)]
pub enum ExternalEvent {
    Sqmr(sqmr::behaviour::ExternalEvent),
    GossipSub(gossipsub_impl::ExternalEvent),
}

#[derive(Debug)]
pub enum ToOtherBehaviourEvent {
    NoOp,
    Identify(IdentifyToOtherBehaviourEvent),
    Kad(KadToOtherBehaviourEvent),
    Discovery(discovery::ToOtherBehaviourEvent),
    PeerManager(peer_manager::ToOtherBehaviourEvent),
    Sqmr(sqmr::ToOtherBehaviourEvent),
}

pub trait BridgedBehaviour {
    fn on_other_behaviour_event(&mut self, event: &ToOtherBehaviourEvent);
}

impl MixedBehaviour {
    // TODO: get config details from network manager config
    /// Panics if bootstrap_peer_multiaddr doesn't have a peer id.
    pub fn new(
        keypair: Keypair,
        bootstrap_peer_multiaddr: Option<Multiaddr>,
        streamed_bytes_config: sqmr::Config,
    ) -> Self {
        let public_key = keypair.public();
        let local_peer_id = PeerId::from_public_key(&public_key);
        Self {
            peer_manager: peer_manager::PeerManager::new(PeerManagerConfig::default()),
            discovery: bootstrap_peer_multiaddr
                .map(|bootstrap_peer_multiaddr| {
                    discovery::Behaviour::new(
                        DialOpts::from(bootstrap_peer_multiaddr.clone())
                            .get_peer_id()
                            .expect("bootstrap_peer_multiaddr doesn't have a peer id"),
                        bootstrap_peer_multiaddr.clone(),
                    )
                })
                .into(),
            identify: identify::Behaviour::new(identify::Config::new(
                IDENTIFY_PROTOCOL_VERSION.to_string(),
                public_key,
            )),
            // TODO: change kademlia protocol name
            kademlia: kad::Behaviour::with_config(
                local_peer_id,
                MemoryStore::new(local_peer_id),
                kad::Config::default()
                    .set_protocol_names(vec![StreamProtocol::new("/starknet/juno-papyrus")])
                    .clone(),
            ),
            sqmr: sqmr::Behaviour::new(streamed_bytes_config),
            gossipsub: gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(keypair),
                gossipsub::Config::default(),
            )
            .unwrap_or_else(|err_string| {
                panic!(
                    "Failed creating gossipsub behaviour due to the following error: {err_string}"
                )
            }),
        }
    }
}
