// TODO(shahak): add flow test

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::{FutureExt, Stream, StreamExt};
use libp2p::swarm::{NetworkBehaviour, ToSwarm};
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::timeout;
use void::Void;

use super::{Behaviour, FromOtherBehaviourEvent, RequestKadQuery};
use crate::main_behaviour::mixed_behaviour;
use crate::main_behaviour::mixed_behaviour::BridgedBehaviour;
use crate::test_utils::next_on_mutex_stream;

const TIMEOUT: Duration = Duration::from_secs(5);
const SLEEP_DURATION: Duration = Duration::from_millis(10);

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<RequestKadQuery, Void>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

#[tokio::test]
async fn discovery_outputs_single_query_on_start() {
    let mut behaviour = Behaviour::new();

    timeout(TIMEOUT, behaviour.next()).await.unwrap();

    assert!(behaviour.next().now_or_never().is_none());
}

#[tokio::test]
async fn discovery_doesnt_output_queries_while_paused() {
    let mut behaviour = Behaviour::new();

    behaviour.on_other_behaviour_event(mixed_behaviour::InternalEvent::NotifyDiscovery(
        FromOtherBehaviourEvent::PauseDiscovery,
    ));
    assert!(behaviour.next().now_or_never().is_none());

    behaviour.on_other_behaviour_event(mixed_behaviour::InternalEvent::NotifyDiscovery(
        FromOtherBehaviourEvent::ResumeDiscovery,
    ));
    timeout(TIMEOUT, behaviour.next()).await.unwrap();
}

#[tokio::test]
async fn discovery_outputs_single_query_on_query_finished() {
    let mut behaviour = Behaviour::new();

    // Consume the initial query event.
    behaviour.next().await;

    behaviour.on_other_behaviour_event(mixed_behaviour::InternalEvent::NotifyDiscovery(
        FromOtherBehaviourEvent::KadQueryFinished,
    ));
    timeout(TIMEOUT, behaviour.next()).await.unwrap();
}

#[tokio::test]
async fn discovery_doesnt_output_queries_if_query_finished_while_paused() {
    let mut behaviour = Behaviour::new();

    // Consume the initial query event.
    behaviour.next().await;

    behaviour.on_other_behaviour_event(mixed_behaviour::InternalEvent::NotifyDiscovery(
        FromOtherBehaviourEvent::PauseDiscovery,
    ));
    assert!(behaviour.next().now_or_never().is_none());

    // Simulate that the query has finished.
    behaviour.on_other_behaviour_event(mixed_behaviour::InternalEvent::NotifyDiscovery(
        FromOtherBehaviourEvent::KadQueryFinished,
    ));
    assert!(behaviour.next().now_or_never().is_none());
}

#[tokio::test]
async fn discovery_awakes_on_resume() {
    let mut behaviour = Behaviour::new();

    behaviour.on_other_behaviour_event(mixed_behaviour::InternalEvent::NotifyDiscovery(
        FromOtherBehaviourEvent::PauseDiscovery,
    ));

    // There should be an event once we resume because discovery has just started.

    let mutex = Mutex::new(behaviour);

    select! {
        _ = async {
            tokio::time::sleep(SLEEP_DURATION).await;
            mutex.lock().await.on_other_behaviour_event(
                mixed_behaviour::InternalEvent::NotifyDiscovery(
                    FromOtherBehaviourEvent::PauseDiscovery,
                )
            );
        } => {},
        maybe_event = next_on_mutex_stream(&mutex) => assert!(maybe_event.is_some()),
    }
}

#[tokio::test]
async fn discovery_awakes_on_query_finished() {
    let mut behaviour = Behaviour::new();

    // Consume the initial query event.
    behaviour.next().await;

    let mutex = Mutex::new(behaviour);

    select! {
        _ = async {
            tokio::time::sleep(SLEEP_DURATION).await;
            mutex.lock().await.on_other_behaviour_event(
                mixed_behaviour::InternalEvent::NotifyDiscovery(
                    FromOtherBehaviourEvent::KadQueryFinished,

                )
            );
        } => {},
        maybe_event = next_on_mutex_stream(&mutex) => assert!(maybe_event.is_some()),
    }
}
