use std::collections::{HashSet, VecDeque};
use std::iter;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::sleep;
use std::time::{Duration, Instant};

use async_std::io;
use async_trait::async_trait;
use futures::future::{poll_fn, BoxFuture};
use futures::prelude::{AsyncRead, AsyncWrite};
use futures::stream::Next;
use futures::StreamExt;
use libp2p::core::identity::Keypair;
use libp2p::core::multiaddr;
use libp2p::core::multihash::Multihash;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::dummy::DummyTransport;
use libp2p::core::transport::{Boxed, MemoryTransport};
use libp2p::core::upgrade::{
    read_varint, write_varint, InboundUpgrade, OutboundUpgrade, ProtocolName, UpgradeInfo, Version,
};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::record::Key;
use libp2p::kad::store::RecordStore;
use libp2p::kad::{
    GetProvidersOk, GetRecordOk, Kademlia, KademliaEvent, QueryInfo, QueryResult, Quorum, Record,
};
use libp2p::noise::NoiseAuthenticated;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::handler::ConnectionEvent;
use libp2p::swarm::{
    ConnectionHandler, ConnectionHandlerEvent, ConnectionId, FromSwarm, KeepAlive,
    NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters, SubstreamProtocol,
    Swarm, SwarmEvent,
};
use libp2p::yamux::YamuxConfig;
use libp2p::{request_response, Multiaddr, PeerId, Transport};
use rand::random;
use tokio::join;

use crate::{Discovery, DiscoveryClient, DiscoveryCodec, ProtocolId};

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
    let (transport2, peer_id2) = get_transport_and_peer_id();
    let address0: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    let address1: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    let address2: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    print!("Peer id 0: {:?}\n", peer_id0);
    print!("Peer id 1: {:?}\n", peer_id1);
    print!("Peer id 2: {:?}\n", peer_id2);
    let receiver0 =
        Discovery::spawn(transport0, peer_id0, address0.clone(), peer_id1, address1.clone()).await;
    let receiver1 =
        Discovery::spawn(transport1, peer_id1, address1.clone(), peer_id2, address2.clone()).await;
    let receiver2 =
        Discovery::spawn(transport2, peer_id2, address2.clone(), peer_id1, address1.clone()).await;
    print!("Peer 0 got peer id: {:?}\n", receiver0.recv());
    print!("Peer 1 got peer id: {:?}\n", receiver1.recv());
    print!("Peer 2 got peer id: {:?}\n", receiver2.recv());
}
