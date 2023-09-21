mod get_stream;

use libp2p::core::transport::memory::MemoryTransport;
use libp2p::core::transport::Transport;
use libp2p::core::{multiaddr, upgrade};
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent};
use libp2p::{noise, yamux, Multiaddr, Stream, Swarm};
use rand::random;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt as TokioStreamExt;

use crate::messages::block::{BlockHeader, GetBlocksResponse};
use crate::messages::common::{BlockId, Fin};
use crate::messages::proto::p2p::proto::get_blocks_response::Response;

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

/// Create two streams that are connected to each other. Return them and a join handle for a thread
/// that will perform the sends between the streams (this thread will run forever so it shouldn't
/// be joined).
pub(crate) async fn get_connected_streams() -> (Stream, Stream, JoinHandle<()>) {
    let (mut dialer_swarm, _) = create_swarm(get_stream::Behaviour::default());
    let (listener_swarm, listener_address) = create_swarm(get_stream::Behaviour::default());
    dialer_swarm
        .dial(
            DialOpts::peer_id(*listener_swarm.local_peer_id())
                .addresses(vec![listener_address])
                .build(),
        )
        .unwrap();
    let merged_swarm = dialer_swarm.merge(listener_swarm);
    let mut filtered_swarm = TokioStreamExt::filter_map(merged_swarm, |event| {
        if let SwarmEvent::Behaviour(stream) = event { Some(stream) } else { None }
    });
    (
        TokioStreamExt::next(&mut filtered_swarm).await.unwrap(),
        TokioStreamExt::next(&mut filtered_swarm).await.unwrap(),
        tokio::task::spawn(async move {
            while TokioStreamExt::next(&mut filtered_swarm).await.is_some() {}
        }),
    )
}

pub(crate) fn hardcoded_data() -> Vec<GetBlocksResponse> {
    vec![
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 1 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 2 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 3 }),
                ..Default::default()
            })),
        },
        GetBlocksResponse { response: Some(Response::Fin(Fin {})) },
    ]
}
