mod get_stream;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use futures::future::{Either, Future};
use futures::pin_mut;
use futures::stream::Stream as StreamTrait;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{ConnectionId, NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::{PeerId, Stream, StreamProtocol};
use libp2p_swarm_test::SwarmExt;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::sqmr::Bytes;
use crate::utils::StreamHashMap;

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
    let mut filtered_swarm = merged_swarm.filter_map(|event| {
        if let SwarmEvent::Behaviour(stream) = event { Some(stream) } else { None }
    });
    (
        filtered_swarm.next().await.unwrap(),
        filtered_swarm.next().await.unwrap(),
        tokio::task::spawn(async move { while filtered_swarm.next().await.is_some() {} }),
    )
}

pub(crate) fn dummy_data() -> Vec<Bytes> {
    vec![vec![1u8], vec![2u8, 3u8], vec![4u8, 5u8, 6u8]]
}

impl crate::sqmr::Config {
    pub fn get_test_config() -> Self {
        Self { session_timeout: Duration::MAX }
    }
}
// TODO(eitan): create a lazy static constant of SUPPORTED_PROTOCOLS which is this vec
impl crate::sqmr::handler::Handler {
    pub fn get_test_supported_protocols() -> HashSet<StreamProtocol> {
        let mut protocols = HashSet::new();
        protocols.insert(StreamProtocol::new("/"));
        protocols
    }
}

/// Create num_swarms swarms and connect each pair of swarms. Return them as a combined stream of
/// events. Also return all the connection ids of the created connections
pub(crate) async fn create_fully_connected_swarms_stream<TBehaviour: NetworkBehaviour + Send>(
    num_swarms: usize,
    behaviour_gen: impl Fn() -> TBehaviour,
) -> (StreamHashMap<PeerId, Swarm<TBehaviour>>, HashMap<(PeerId, PeerId), ConnectionId>)
where
    <TBehaviour as NetworkBehaviour>::ToSwarm: Debug,
{
    let mut swarms =
        (0..num_swarms).map(|_| Swarm::new_ephemeral(|_| behaviour_gen())).collect::<Vec<_>>();

    for swarm in &mut swarms {
        swarm.listen().with_memory_addr_external().await;
    }

    let mut connection_ids = HashMap::new();

    for i in 0..(swarms.len() - 1) {
        let (swarms1, swarms2) = swarms.split_at_mut(i + 1);
        let swarm1 = &mut swarms1[i];
        let peer_id1 = *swarm1.local_peer_id();
        for swarm2 in swarms2 {
            let (connection_id1, connection_id2) = connect_swarms(swarm1, swarm2).await;
            let peer_id2 = *swarm2.local_peer_id();
            connection_ids.insert((peer_id1, peer_id2), connection_id1);
            connection_ids.insert((peer_id2, peer_id1), connection_id2);
        }
    }

    (
        StreamHashMap::new(
            swarms.into_iter().map(|swarm| (*swarm.local_peer_id(), swarm)).collect(),
        ),
        connection_ids,
    )
}

// Copied from SwarmExt::connect, but this function returns the connection id.
/// Connect two swarms and return the connection id that each swarm gave to this connection.
async fn connect_swarms<TBehaviour: NetworkBehaviour + Send>(
    swarm1: &mut Swarm<TBehaviour>,
    swarm2: &mut Swarm<TBehaviour>,
) -> (ConnectionId, ConnectionId)
where
    <TBehaviour as NetworkBehaviour>::ToSwarm: Debug,
{
    let external_addresses = swarm2.external_addresses().cloned().collect();

    let dial_opts = DialOpts::peer_id(*swarm2.local_peer_id())
        .addresses(external_addresses)
        .condition(PeerCondition::Always)
        .build();

    swarm1.dial(dial_opts).unwrap();

    let mut dialer_connection_id = None;
    let mut listener_connection_id = None;

    loop {
        match futures::future::select(swarm1.next_swarm_event(), swarm2.next_swarm_event()).await {
            Either::Left((SwarmEvent::ConnectionEstablished { connection_id, .. }, _)) => {
                dialer_connection_id = Some(connection_id);
            }
            Either::Right((SwarmEvent::ConnectionEstablished { connection_id, .. }, _)) => {
                listener_connection_id = Some(connection_id);
            }
            Either::Left((swarm2, _)) => {
                tracing::debug!(
                    dialer=?swarm2,
                    "Ignoring event from dialer"
                );
            }
            Either::Right((swarm2, _)) => {
                tracing::debug!(
                    listener=?swarm2,
                    "Ignoring event from listener"
                );
            }
        }

        if let Some((dialer_connection_id, listener_connection_id)) =
            dialer_connection_id.zip(listener_connection_id)
        {
            return (dialer_connection_id, listener_connection_id);
        }
    }
}

// I tried making this generic on the async function we run, but it caused a lot of lifetime
// issues.
/// Run `next` on a mutex of a stream, unlocking it while the function is pending.
pub(crate) fn next_on_mutex_stream<T: StreamTrait + Unpin>(
    mutex: &Mutex<T>,
) -> NextOnMutexStream<'_, T> {
    NextOnMutexStream { mutex }
}

pub(crate) struct NextOnMutexStream<'a, T: StreamTrait + Unpin> {
    mutex: &'a Mutex<T>,
}

impl<'a, T: StreamTrait + Unpin> Future for NextOnMutexStream<'a, T> {
    type Output = Option<T::Item>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let lock_fut = self.mutex.lock();
        pin_mut!(lock_fut);
        let mut locked_value = ready!(lock_fut.poll(cx));
        let fut = StreamExt::next(&mut *locked_value);
        pin_mut!(fut);
        fut.poll(cx)
    }
}
