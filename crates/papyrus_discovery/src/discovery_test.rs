use std::collections::HashSet;

use async_std::stream::StreamExt;
use libp2p::core::identity::{Keypair, PublicKey};
use libp2p::core::multiaddr;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::{Boxed, MemoryTransport};
use libp2p::core::upgrade::Version;
use libp2p::noise::NoiseAuthenticated;
use libp2p::yamux::YamuxConfig;
use libp2p::{Multiaddr, PeerId, Transport};
use rand::random;

use crate::Discovery;

fn get_transport_and_public_key() -> (Boxed<(PeerId, StreamMuxerBox)>, PublicKey) {
    let key_pair = Keypair::generate_ed25519();
    let transport = MemoryTransport::default()
        .upgrade(Version::V1)
        .authenticate(NoiseAuthenticated::xx(&key_pair).unwrap())
        .multiplex(YamuxConfig::default())
        .boxed();

    let public_key = key_pair.public();
    (transport, public_key)
}

#[tokio::test]
async fn basic_usage() {
    let (transport0, public_key0) = get_transport_and_public_key();
    let (transport1, public_key1) = get_transport_and_public_key();
    let (transport2, public_key2) = get_transport_and_public_key();
    let peer_id0 = public_key0.to_peer_id();
    let peer_id1 = public_key1.to_peer_id();
    let peer_id2 = public_key2.to_peer_id();
    let address0: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    let address1: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    let address2: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    let discovery0 =
        Discovery::new(transport0, public_key0, address0.clone(), peer_id2, address2.clone());
    let discovery1 =
        Discovery::new(transport1, public_key1, address1.clone(), peer_id0, address0.clone());
    let discovery2 =
        Discovery::new(transport2, public_key2, address2.clone(), peer_id1, address1.clone());
    let merged_stream = discovery0
        .map(|x| (0, x))
        .merge(discovery1.map(|x| (1, x)))
        .merge(discovery2.map(|x| (2, x)));
    let result: HashSet<(u64, PeerId)> = merged_stream.take(6).collect().await;
    let mut expected_result = HashSet::new();
    expected_result.insert((0, peer_id1));
    expected_result.insert((0, peer_id2));
    expected_result.insert((1, peer_id0));
    expected_result.insert((1, peer_id2));
    expected_result.insert((2, peer_id0));
    expected_result.insert((2, peer_id1));
    assert_eq!(result, expected_result)
}
