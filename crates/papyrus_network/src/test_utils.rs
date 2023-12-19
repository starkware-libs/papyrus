mod get_stream;

use std::fmt::Debug;

use libp2p::core::multiaddr;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::{Multiaddr, Stream};
use libp2p_swarm_test::SwarmExt;
use rand::random;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt as TokioStreamExt;

use crate::messages::block::{BlockHeader, BlockHeadersResponse};
use crate::messages::common::Fin;
use crate::messages::proto::p2p::proto::{block_headers_response_part, BlockHeadersResponsePart};

pub(crate) fn create_swarm<BehaviourT: NetworkBehaviour + Send>(
    behaviour: BehaviourT,
) -> (Swarm<BehaviourT>, Multiaddr)
where
    <BehaviourT as NetworkBehaviour>::ToSwarm: Debug,
{
    let mut swarm = Swarm::new_ephemeral(|_| behaviour);

    // Using a random address because if two different tests use the same address simultaneously
    // they will fail.
    let listen_address: Multiaddr = multiaddr::Protocol::Memory(random::<u64>()).into();
    swarm.listen_on(listen_address.clone()).unwrap();
    swarm.add_external_address(listen_address.clone());
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

pub(crate) fn hardcoded_data() -> Vec<BlockHeadersResponse> {
    vec![
        BlockHeadersResponse {
            part: vec![BlockHeadersResponsePart {
                header_message: Some(block_headers_response_part::HeaderMessage::Header(
                    BlockHeader { number: 1, ..Default::default() },
                )),
            }],
        },
        BlockHeadersResponse {
            part: vec![BlockHeadersResponsePart {
                header_message: Some(block_headers_response_part::HeaderMessage::Header(
                    BlockHeader { number: 2, ..Default::default() },
                )),
            }],
        },
        BlockHeadersResponse {
            part: vec![BlockHeadersResponsePart {
                header_message: Some(block_headers_response_part::HeaderMessage::Header(
                    BlockHeader { number: 3, ..Default::default() },
                )),
            }],
        },
        BlockHeadersResponse {
            part: vec![BlockHeadersResponsePart {
                header_message: Some(block_headers_response_part::HeaderMessage::Fin(
                    Fin::default(),
                )),
            }],
        },
    ]
}
