use std::collections::HashSet;
use std::iter;
use std::pin::Pin;
use std::task::Poll;

use futures::{Stream, StreamExt};
use libp2p::core::identity::{Keypair, PublicKey};
use libp2p::core::multiaddr;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::{Boxed, MemoryTransport};
use libp2p::core::upgrade::Version;
use libp2p::noise::NoiseAuthenticated;
use libp2p::yamux::YamuxConfig;
use libp2p::{Multiaddr, PeerId, Transport};
use rand::random;

use crate::{Discovery, DiscoveryConfig};

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

// TODO extract to a utility.
struct MergedStream<S>
where
    S: StreamExt + Unpin,
{
    streams: Vec<S>,
    is_stream_consumed_vec: Vec<bool>,
}

impl<S> Unpin for MergedStream<S> where S: StreamExt + Unpin {}

impl<S> Stream for MergedStream<S>
where
    S: StreamExt + Unpin,
{
    type Item = (usize, S::Item);
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        for ((i, stream), is_consumed) in unpinned_self
            .streams
            .iter_mut()
            .enumerate()
            .zip(unpinned_self.is_stream_consumed_vec.iter_mut())
        {
            if *is_consumed {
                continue;
            }
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(item)) => {
                    return Poll::Ready(Some((i, item)));
                }
                Poll::Ready(None) => {
                    *is_consumed = true;
                    continue;
                }
                Poll::Pending => {
                    continue;
                }
            }
        }
        if unpinned_self.is_stream_consumed_vec.iter().all(|x| *x) {
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}

impl<S> MergedStream<S>
where
    S: StreamExt + Unpin,
{
    pub fn new(streams: Vec<S>) -> Self {
        let len = streams.len();
        Self { streams, is_stream_consumed_vec: iter::repeat(false).take(len).collect() }
    }
}

async fn test_graph<I>(graph: Vec<I>)
where
    I: IntoIterator<Item = usize>,
{
    let n_vertices = graph.len();
    let transports_and_public_keys: Vec<(Boxed<(PeerId, StreamMuxerBox)>, PublicKey)> =
        (0..n_vertices).map(|_| get_transport_and_public_key()).collect();
    let addresses: Vec<Multiaddr> =
        (0..n_vertices).map(|_| multiaddr::Protocol::Memory(random::<u64>()).into()).collect();
    let peer_ids: Vec<PeerId> =
        transports_and_public_keys.iter().map(|(_, public_key)| public_key.to_peer_id()).collect();
    let discoveries: Vec<Discovery> = graph
        .into_iter()
        .zip(transports_and_public_keys.into_iter().zip(addresses.iter()))
        .map(|(out_vertices, ((transport, public_key), address))| {
            Discovery::new(
                DiscoveryConfig::default(),
                transport,
                public_key.clone(),
                address.clone(),
                out_vertices.into_iter().map(|i| (peer_ids[i], addresses[i].clone())),
            )
        })
        .collect();
    let stream = MergedStream::new(discoveries);
    let result: HashSet<(usize, PeerId)> =
        stream.take(n_vertices * (n_vertices - 1)).collect().await;
    let expected_result: HashSet<(usize, PeerId)> = (0..n_vertices)
        .flat_map(|i| peer_ids.iter().cloned().map(move |peer_id| (i, peer_id)))
        .filter(|(i, peer_id)| *peer_id != peer_ids[*i])
        .collect();
    assert_eq!(result, expected_result);
}

#[tokio::test]
async fn basic_usage_chain() {
    test_graph((0..10).map(|i| vec![if i == 0 { 1 } else { i - 1 }]).collect()).await;
}

#[tokio::test]
async fn basic_usage_two_stars() {
    test_graph((0..10).map(|i| vec![if i < 2 { 1 - i } else { i % 2 }]).collect()).await;
}
