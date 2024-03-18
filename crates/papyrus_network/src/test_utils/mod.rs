mod get_stream;

use std::collections::hash_map::{Keys, ValuesMut};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::stream::{Stream as StreamTrait, StreamExt};
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, Swarm, SwarmEvent};
use libp2p::{PeerId, Stream};
use libp2p_swarm_test::SwarmExt;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt as TokioStreamExt;

use crate::streamed_bytes::Bytes;

/// Create two streams that are connected to each other. Return them and a join handle for a thread
/// that will perform the sends between the streams (this thread will run forever so it shouldn't
/// be joined).
pub(crate) async fn get_connected_streams() -> (Stream, Stream, JoinHandle<()>) {
    let mut swarm1 = Swarm::new_ephemeral(|_| get_stream::Behaviour::default());
    let mut swarm2 = Swarm::new_ephemeral(|_| get_stream::Behaviour::default());
    swarm1.listen().with_memory_addr_external().await;
    swarm2.listen().with_memory_addr_external().await;

    swarm1.connect(&mut swarm2).await;

    let merged_swarm = swarm1.merge(swarm2);
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

pub(crate) fn dummy_data() -> Vec<Bytes> {
    vec![vec![1u8], vec![2u8, 3u8], vec![4u8, 5u8, 6u8]]
}

impl crate::streamed_bytes::Config {
    pub fn get_test_config() -> Self {
        Self {
            session_timeout: Duration::MAX,
            supported_inbound_protocols: vec![StreamProtocol::new("/")],
        }
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

    #[allow(dead_code)]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
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
