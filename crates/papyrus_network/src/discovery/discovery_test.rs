// TODO(shahak): add flow test

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use assert_matches::assert_matches;
use futures::future::pending;
use futures::{FutureExt, Stream, StreamExt};
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    ConnectionClosed,
    ConnectionId,
    DialError,
    DialFailure,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::timeout;
use void::Void;

use super::kad_impl::KadToOtherBehaviourEvent;
use super::{Behaviour, ToOtherBehaviourEvent, DIAL_SLEEP};
use crate::mixed_behaviour::BridgedBehaviour;
use crate::test_utils::next_on_mutex_stream;
use crate::{mixed_behaviour, peer_manager};

const TIMEOUT: Duration = Duration::from_secs(1);
const SLEEP_DURATION: Duration = Duration::from_millis(10);

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<ToOtherBehaviourEvent, Void>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

// In case we have a bug when we return pending and then return an event.
const TIMES_TO_CHECK_FOR_PENDING_EVENT: usize = 5;

fn assert_no_event(behaviour: &mut Behaviour) {
    for _ in 0..TIMES_TO_CHECK_FOR_PENDING_EVENT {
        assert!(behaviour.next().now_or_never().is_none());
    }
}

#[tokio::test]
async fn discovery_outputs_dial_request_on_start_without_query() {
    let bootstrap_peer_id = PeerId::random();
    let bootstrap_peer_address = Multiaddr::empty();

    let mut behaviour = Behaviour::new(bootstrap_peer_id, bootstrap_peer_address);

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    assert_no_event(&mut behaviour);
}

#[tokio::test]
async fn discovery_redials_on_dial_failure() {
    const EPSILON_SLEEP: Duration = Duration::from_millis(10);

    let bootstrap_peer_id = PeerId::random();
    let bootstrap_peer_address = Multiaddr::empty();

    let mut behaviour = Behaviour::new(bootstrap_peer_id, bootstrap_peer_address);

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );

    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(bootstrap_peer_id),
        error: &DialError::Aborted,
        connection_id: ConnectionId::new_unchecked(0),
    }));

    // Check that there are no events until we sleep for enough time.
    tokio::time::pause();
    tokio::time::advance(DIAL_SLEEP - EPSILON_SLEEP).await;
    assert_no_event(&mut behaviour);

    // Sleep and check for event.
    tokio::time::advance(EPSILON_SLEEP).await;
    tokio::time::resume();
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(bootstrap_peer_id)
    );
}

#[tokio::test]
async fn discovery_redials_when_all_connections_closed() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: behaviour.bootstrap_peer_id(),
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: behaviour.bootstrap_peer_address().clone(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 0,
    }));

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::Dial{opts} if opts.get_peer_id() == Some(behaviour.bootstrap_peer_id())
    );
}

#[tokio::test]
async fn discovery_doesnt_redial_when_one_connection_closes() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: behaviour.bootstrap_peer_id(),
        connection_id: ConnectionId::new_unchecked(1),
        endpoint: &ConnectedPoint::Dialer {
            address: behaviour.bootstrap_peer_address().clone(),
            role_override: Endpoint::Dialer,
        },
        failed_addresses: &[],
        other_established: 1,
    }));

    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id: behaviour.bootstrap_peer_id(),
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: behaviour.bootstrap_peer_address().clone(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 1,
    }));

    assert_no_event(&mut behaviour);
}

async fn create_behaviour_and_connect_to_bootstrap_node() -> Behaviour {
    let bootstrap_peer_id = PeerId::random();
    let bootstrap_peer_address = Multiaddr::empty();

    let mut behaviour = Behaviour::new(bootstrap_peer_id, bootstrap_peer_address.clone());

    // Consume the dial event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: bootstrap_peer_id,
        connection_id: ConnectionId::new_unchecked(0),
        endpoint: &ConnectedPoint::Dialer {
            address: bootstrap_peer_address.clone(),
            role_override: Endpoint::Dialer,
        },
        failed_addresses: &[],
        other_established: 0,
    }));

    // Consume the event to add the bootstrap node to kademlia.
    // TODO(shahak): Consider extracting the validation to a separate test.
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::FoundListenAddresses {
                peer_id,
                listen_addresses,
            }
        ) if peer_id == bootstrap_peer_id && listen_addresses == vec![bootstrap_peer_address]
    );

    behaviour
}

#[tokio::test]
async fn discovery_outputs_single_query_after_connecting() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );

    assert_no_event(&mut behaviour);
}

#[tokio::test]
async fn discovery_doesnt_output_queries_while_paused() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
        peer_manager::ToOtherBehaviourEvent::PauseDiscovery,
    ));
    assert_no_event(&mut behaviour);

    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
        peer_manager::ToOtherBehaviourEvent::ResumeDiscovery,
    ));
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}

#[tokio::test]
async fn discovery_outputs_single_query_on_query_finished() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::Kad(
        KadToOtherBehaviourEvent::KadQueryFinished,
    ));
    let event = timeout(TIMEOUT, behaviour.next()).await.unwrap().unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::RequestKadQuery(_peer_id))
    );
}

#[tokio::test]
async fn discovery_doesnt_output_queries_if_query_finished_while_paused() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
        peer_manager::ToOtherBehaviourEvent::PauseDiscovery,
    ));
    assert_no_event(&mut behaviour);

    // Simulate that the query has finished.
    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::Kad(
        KadToOtherBehaviourEvent::KadQueryFinished,
    ));
    assert_no_event(&mut behaviour);
}

#[tokio::test]
async fn discovery_awakes_on_resume() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
        peer_manager::ToOtherBehaviourEvent::PauseDiscovery,
    ));

    // There should be an event once we resume because discovery has just started.

    let mutex = Mutex::new(behaviour);

    select! {
        _ = async {
            tokio::time::sleep(SLEEP_DURATION).await;
            mutex.lock().await.on_other_behaviour_event(
                &mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
                    peer_manager::ToOtherBehaviourEvent::ResumeDiscovery,
                )
            );
            timeout(TIMEOUT, pending::<()>()).await.unwrap();
        } => {},
        maybe_event = next_on_mutex_stream(&mutex) => assert!(maybe_event.is_some()),
    }
}

#[tokio::test]
async fn discovery_awakes_on_query_finished() {
    let mut behaviour = create_behaviour_and_connect_to_bootstrap_node().await;

    // Consume the initial query event.
    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    let mutex = Mutex::new(behaviour);

    select! {
        _ = async {
            tokio::time::sleep(SLEEP_DURATION).await;
            mutex.lock().await.on_other_behaviour_event(
                &mixed_behaviour::ToOtherBehaviourEvent::Kad(
                    KadToOtherBehaviourEvent::KadQueryFinished,
                )
            );
            timeout(TIMEOUT, pending::<()>()).await.unwrap();
        } => {},
        maybe_event = next_on_mutex_stream(&mutex) => assert!(maybe_event.is_some()),
    }
}
