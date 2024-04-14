use std::collections::HashSet;
use std::iter;
use std::pin::Pin;
use std::task::Poll;
use std::time::Instant;

use futures::executor::block_on;
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
use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

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
    pub streams: Vec<S>,
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
        loop {
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
        }
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

fn discoveries_stream_from_graph<I>(
    graph: Vec<I>,
    config: DiscoveryConfig,
) -> MergedStream<Discovery>
where
    I: IntoIterator<Item = usize>,
{
    let fmt_layer = fmt::layer().compact().without_time().with_level(false).with_target(false);
    let level_filter_layer =
        EnvFilter::builder().with_default_directive(LevelFilter::ERROR.into()).from_env_lossy();

    // This sets a single subscriber to all of the threads. We may want to implement different
    // subscriber for some threads and use set_global_default instead of init.
    tracing_subscriber::registry().with(fmt_layer).with(level_filter_layer).init();
    let n_vertices = graph.len();
    let transports_and_public_keys: Vec<(Boxed<(PeerId, StreamMuxerBox)>, PublicKey)> =
        (0..n_vertices).map(|_| get_transport_and_public_key()).collect();
    let addresses: Vec<Multiaddr> =
        (0..n_vertices).map(|_| multiaddr::Protocol::Memory(random::<u64>()).into()).collect();
    let peer_ids: Vec<PeerId> =
        transports_and_public_keys.iter().map(|(_, public_key)| public_key.to_peer_id()).collect();
    let global_peers_names: Vec<_> = peer_ids
        .iter()
        .cloned()
        .zip(addresses.iter().cloned())
        .enumerate()
        .map(|(x, (y, z))| (x.to_string(), y, z))
        .collect();
    let discoveries: Vec<Discovery> = graph
        .into_iter()
        .zip(transports_and_public_keys.into_iter().zip(addresses.iter()))
        .map(|(out_vertices, ((transport, public_key), address))| {
            Discovery::new(
                config.clone(),
                transport,
                public_key.clone(),
                address.clone(),
                out_vertices.into_iter().map(|i| (peer_ids[i], addresses[i].clone())),
                global_peers_names.clone(),
            )
        })
        .collect();
    MergedStream::new(discoveries)
}

fn test_found_all_peers(stream: MergedStream<Discovery>) {
    let peer_ids: Vec<PeerId> =
        stream.streams.iter().map(|discovery| discovery.peer_id().clone()).collect();
    let n_peers = peer_ids.len();
    let result: HashSet<(usize, PeerId)> = block_on(stream.take(n_peers * (n_peers + 1)).collect());
    let expected_result: HashSet<(usize, PeerId)> = (0..n_peers)
        .flat_map(|i| peer_ids.iter().cloned().map(move |peer_id| (i, peer_id)))
        .filter(|(i, peer_id)| *peer_id != peer_ids[*i])
        .collect();
    assert_eq!(result, expected_result);
}

#[test]
fn basic_usage_chain() {
    test_found_all_peers(discoveries_stream_from_graph(
        (0..6).map(|i| vec![if i == 0 { 1 } else { i - 1 }]).collect(),
        DiscoveryConfig::default(),
    ));
}

#[test]
fn basic_usage_two_stars() {
    test_found_all_peers(discoveries_stream_from_graph(
        (0..50).map(|i| vec![if i < 2 { 1 - i } else { i % 2 }]).collect(),
        DiscoveryConfig::default(),
    ));
}

#[test]
fn bench_chain() {
    const N_NODES: usize = 20;
    // const FOUND_PEERS_LIMIT: usize = 9;
    let mut stream = discoveries_stream_from_graph(
        (0..N_NODES).map(|i| vec![if i == 0 { 1 } else { i - 1 }]).collect(),
        // DiscoveryConfig { n_active_queries: 1, found_peers_limit: Some(FOUND_PEERS_LIMIT) },
        DiscoveryConfig::default(),
    );
    let start_time = Instant::now();
    // while let Some(_) = block_on(stream.next()) {}
    for _ in 0..(N_NODES * (N_NODES - 1)) {
        block_on(stream.next());
    }
    println!("Took {} ms", start_time.elapsed().as_millis());
}
