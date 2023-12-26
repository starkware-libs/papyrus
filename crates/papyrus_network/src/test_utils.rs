mod get_stream;

use std::collections::hash_map::{Keys, ValuesMut};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::stream::{Stream as StreamTrait, StreamExt};
use libp2p::core::multiaddr;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, Swarm, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_swarm_test::SwarmExt;
use rand::random;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt as TokioStreamExt;

use crate::messages::protobuf;

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

pub(crate) fn hardcoded_data() -> Vec<protobuf::BasicMessage> {
    vec![
        protobuf::BasicMessage { number: 1 },
        protobuf::BasicMessage { number: 2 },
        protobuf::BasicMessage { number: 3 },
    ]
}

impl crate::streamed_data_protocol::Config {
    pub fn get_test_config() -> Self {
        Self { substream_timeout: Duration::MAX, protocol_name: StreamProtocol::new("/") }
    }
}

// This is an implementation of `StreamMap` from tokio_stream. The reason we're implementing it
// ourselves is that the implementation in tokio_stream requires that the values implement the
// Stream trait from tokio_stream and not from futures.
pub(crate) struct StreamHashMap<K: Unpin + Clone + Eq + Hash, V: StreamTrait + Unpin> {
    map: HashMap<K, V>,
    finished_streams: HashSet<K>,
}

impl<K: Unpin + Clone + Eq + Hash, V: StreamTrait + Unpin> StreamHashMap<K, V> {
    pub fn new(map: HashMap<K, V>) -> Self {
        Self { map, finished_streams: Default::default() }
    }

    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        self.map.values_mut()
    }

    pub fn keys(&self) -> Keys<'_, K, V> {
        self.map.keys()
    }
}

impl<K: Unpin + Clone + Eq + Hash, V: StreamTrait + Unpin> StreamTrait for StreamHashMap<K, V> {
    type Item = (K, <V as StreamTrait>::Item);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        let mut finished = true;
        for (key, stream) in &mut unpinned_self.map {
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(value)) => {
                    return Poll::Ready(Some((key.clone(), value)));
                }
                Poll::Ready(None) => {
                    unpinned_self.finished_streams.insert(key.clone());
                }
                Poll::Pending => {
                    finished = false;
                }
            }
        }
        if finished {
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}

/// Create num_swarms swarms and connect each pair of swarms. Return them as a combined stream of
/// events.
pub(crate) async fn create_fully_connected_swarms_stream<TBehaviour: NetworkBehaviour + Send>(
    num_swarms: usize,
    behaviour_gen: impl Fn() -> TBehaviour,
) -> StreamHashMap<PeerId, Swarm<TBehaviour>>
where
    <TBehaviour as NetworkBehaviour>::ToSwarm: Debug,
{
    let mut swarms =
        (0..num_swarms).map(|_| Swarm::new_ephemeral(|_| behaviour_gen())).collect::<Vec<_>>();

    for swarm in &mut swarms {
        swarm.listen().with_memory_addr_external().await;
    }

    for i in 0..(swarms.len() - 1) {
        let (swarms1, swarms2) = swarms.split_at_mut(i + 1);
        let swarm1 = &mut swarms1[i];
        for swarm2 in swarms2 {
            swarm1.connect(swarm2).await;
        }
    }

    StreamHashMap::new(swarms.into_iter().map(|swarm| (*swarm.local_peer_id(), swarm)).collect())
}
