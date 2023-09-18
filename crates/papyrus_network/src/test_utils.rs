use futures::{AsyncRead, AsyncWrite, StreamExt};
use libp2p::core::multiaddr::multiaddr;
use libp2p::core::transport::memory::MemoryTransport;
use libp2p::core::transport::{ListenerId, Transport};
use libp2p::core::{multiaddr, upgrade};
use libp2p::identity::Keypair;
use libp2p::swarm::{NetworkBehaviour, SwarmBuilder};
use libp2p::{noise, yamux, Multiaddr, Swarm};
use rand::random;

// TODO(shahak): Use create_swarm and remove allow(dead_code)
#[allow(dead_code)]
pub(crate) fn create_swarm<BehaviourT: NetworkBehaviour>(
    behaviour: BehaviourT,
) -> (Swarm<BehaviourT>, Multiaddr) {
    let key_pair = Keypair::generate_ed25519();
    let public_key = key_pair.public();
    let transport = MemoryTransport::default()
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&key_pair).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();

    let peer_id = public_key.to_peer_id();
    let mut swarm = SwarmBuilder::without_executor(transport, behaviour, peer_id).build();

    // Using a random address because if two different tests use the same address simultaneously
    // they will fail.
    let listen_address: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    swarm.listen_on(listen_address.clone()).unwrap();
    (swarm, listen_address)
}

pub(crate) async fn get_connected_streams()
-> (impl AsyncRead + AsyncWrite, impl AsyncRead + AsyncWrite) {
    let address = multiaddr![Memory(0u64)];
    let mut transport = MemoryTransport::new().boxed();
    transport.listen_on(ListenerId::next(), address).unwrap();
    let listener_addr = transport
        .select_next_some()
        .await
        .into_new_address()
        .expect("MemoryTransport not listening on an address!");

    tokio::join!(
        async move {
            let transport_event = transport.next().await.unwrap();
            let (listener_upgrade, _) = transport_event.into_incoming().unwrap();
            listener_upgrade.await.unwrap()
        },
        async move { MemoryTransport::new().dial(listener_addr).unwrap().await.unwrap() },
    )
}
