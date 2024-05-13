use std::collections::HashSet;
use std::iter;

use futures::StreamExt;
use libp2p::core::multiaddr::Protocol;
use libp2p::identity::PublicKey;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, kad, Multiaddr, Swarm};
use libp2p_swarm_test::SwarmExt;

use super::Behaviour;
use crate::mixed_behaviour;
use crate::mixed_behaviour::{BridgedBehaviour, MixedBehaviour};
use crate::test_utils::StreamHashMap;

#[derive(NetworkBehaviour)]
struct DiscoveryMixedBehaviour {
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub discovery: Toggle<Behaviour>,
}

impl DiscoveryMixedBehaviour {
    pub fn new(key: PublicKey, bootstrap_peer_multiaddr: Option<Multiaddr>) -> Self {
        let mixed_behaviour =
            MixedBehaviour::new(key, bootstrap_peer_multiaddr, Default::default());
        Self {
            identify: mixed_behaviour.identify,
            kademlia: mixed_behaviour.kademlia,
            discovery: mixed_behaviour.discovery,
        }
    }
}

#[tokio::test]
async fn all_nodes_have_same_bootstrap_peer() {
    const NUM_NODES: usize = 2;

    let mut bootstrap_swarm =
        Swarm::new_ephemeral(|keypair| DiscoveryMixedBehaviour::new(keypair.public(), None));
    bootstrap_swarm.listen().with_memory_addr_external().await;

    let bootstrap_peer_id = *bootstrap_swarm.local_peer_id();
    let bootstrap_peer_multiaddr = bootstrap_swarm
        .external_addresses()
        .next()
        .unwrap()
        .clone()
        .with_p2p(bootstrap_peer_id)
        .unwrap();

    let swarms = (0..NUM_NODES).map(|_| {
        Swarm::new_ephemeral(|keypair| {
            DiscoveryMixedBehaviour::new(keypair.public(), Some(bootstrap_peer_multiaddr.clone()))
        })
    });
    let mut swarms_stream = StreamHashMap::new(
        iter::once(bootstrap_swarm)
            .chain(swarms)
            .map(|swarm| (*swarm.local_peer_id(), swarm))
            .collect(),
    );
    for swarm in swarms_stream.values_mut() {
        // Can't use libp2p's listen function since it assumes no other events are emitted.
        swarm.listen_on(Protocol::Memory(0).into()).unwrap();
    }

    let mut connected_peers = HashSet::new();

    while connected_peers.len() < NUM_NODES * (NUM_NODES - 1) {
        let (peer_id, event) = swarms_stream.next().await.unwrap();

        let mixed_event: mixed_behaviour::Event = match event {
            SwarmEvent::Behaviour(DiscoveryMixedBehaviourEvent::Discovery(event)) => event.into(),
            SwarmEvent::Behaviour(DiscoveryMixedBehaviourEvent::Kademlia(event)) => event.into(),
            SwarmEvent::Behaviour(DiscoveryMixedBehaviourEvent::Identify(event)) => event.into(),
            SwarmEvent::ConnectionEstablished { peer_id: other_peer_id, .. } => {
                if peer_id != bootstrap_peer_id && bootstrap_peer_id != other_peer_id {
                    connected_peers.insert((peer_id, other_peer_id));
                }
                continue;
            }
            _ => continue,
        };

        let mixed_behaviour::Event::ToOtherBehaviourEvent(event) = mixed_event else {
            continue;
        };
        if let mixed_behaviour::ToOtherBehaviourEvent::NoOp = event {
            continue;
        };
        let behaviour_ref = swarms_stream.get_mut(&peer_id).unwrap().behaviour_mut();
        behaviour_ref.identify.on_other_behaviour_event(&event);
        behaviour_ref.kademlia.on_other_behaviour_event(&event);
        if let Some(discovery) = behaviour_ref.discovery.as_mut() {
            discovery.on_other_behaviour_event(&event);
        }
    }
}
