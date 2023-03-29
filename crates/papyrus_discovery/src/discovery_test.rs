use std::iter;
use std::sync::{Arc, Mutex};

use libp2p::core::identity::Keypair;
use libp2p::core::multiaddr;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::dummy::DummyTransport;
use libp2p::core::transport::{Boxed, MemoryTransport};
use libp2p::core::upgrade::Version;
use libp2p::noise::NoiseAuthenticated;
use libp2p::swarm::Swarm;
use libp2p::yamux::YamuxConfig;
use libp2p::{request_response, Multiaddr, PeerId, Transport};
use rand::random;

use crate::{DiscoveryClient, DiscoveryCodec, ProtocolId};

fn get_transport_and_peer_id() -> (Boxed<(PeerId, StreamMuxerBox)>, PeerId) {
    let key_pair = Keypair::generate_ed25519();
    let transport = MemoryTransport::default()
        .upgrade(Version::V1)
        .authenticate(NoiseAuthenticated::xx(&key_pair).unwrap())
        .multiplex(YamuxConfig::default())
        .boxed();

    let local_id = key_pair.public().to_peer_id();
    (transport, local_id)
}

#[tokio::test]
async fn basic_usage() {
    let (transport0, peer_id0) = get_transport_and_peer_id();
    let (transport1, peer_id1) = get_transport_and_peer_id();
    let address0: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    let address1: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    // let address1: Multiaddr = "/ip4/127.0.0.1/tcp/10001".parse().unwrap();
    print!("peer_id0: {}\npeer_id1: {}\n", peer_id0, peer_id1);
    let behaviour0 = request_response::Behaviour::new(
        DiscoveryCodec {},
        iter::once((ProtocolId {}, request_response::ProtocolSupport::Full)),
        Default::default(),
    );
    let behaviour1 = request_response::Behaviour::new(
        DiscoveryCodec {},
        iter::once((ProtocolId {}, request_response::ProtocolSupport::Full)),
        Default::default(),
    );
    let mut swarm0 = Swarm::without_executor(transport0, behaviour0, peer_id0);
    let mut swarm1 = Swarm::without_executor(transport1, behaviour1, peer_id1);
    swarm0.listen_on(address0.clone());
    swarm1.listen_on(address1.clone());
    let client0 = DiscoveryClient { other_peer_id: peer_id1, other_peer_address: address1 };
    let client1 = DiscoveryClient { other_peer_id: peer_id0, other_peer_address: address0 };
    tokio::join!(client1.run(swarm1), client0.run(swarm0));
}
