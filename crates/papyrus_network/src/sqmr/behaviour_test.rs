use std::pin::Pin;
use std::task::{Context, Poll};

use assert_matches::assert_matches;
use futures::{FutureExt, Stream, StreamExt};
use lazy_static::lazy_static;
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::{ConnectionClosed, ConnectionId, FromSwarm, NetworkBehaviour, ToSwarm};
use libp2p::{Multiaddr, PeerId, StreamProtocol};

use super::super::handler::{RequestFromBehaviourEvent, RequestToBehaviourEvent};
use super::super::{Bytes, Config, GenericEvent, InboundSessionId, OutboundSessionId, SessionId};
use super::{Behaviour, Event, ExternalEvent, SessionError, ToOtherBehaviourEvent};
use crate::mixed_behaviour::BridgedBehaviour;
use crate::test_utils::dummy_data;
use crate::{mixed_behaviour, peer_manager};

impl Unpin for Behaviour {}

impl Stream for Behaviour {
    type Item = ToSwarm<Event, RequestFromBehaviourEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

lazy_static! {
    static ref QUERY: Bytes = vec![1u8, 2u8, 3u8];
    static ref PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/");
}

fn simulate_peer_assigned(
    behaviour: &mut Behaviour,
    peer_id: PeerId,
    outbound_session_id: OutboundSessionId,
) {
    behaviour.on_other_behaviour_event(&mixed_behaviour::ToOtherBehaviourEvent::PeerManager(
        peer_manager::ToOtherBehaviourEvent::SessionAssigned {
            outbound_session_id,
            peer_id,
            // TODO(shahak): Add test with multiple connections
            connection_id: ConnectionId::new_unchecked(0),
        },
    ));
}

fn simulate_new_inbound_session(
    behaviour: &mut Behaviour,
    peer_id: PeerId,
    inbound_session_id: InboundSessionId,
    query: Bytes,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        // This is the same connection_id from simulate_peer_assigned
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::GenerateEvent(GenericEvent::NewInboundSession {
            query,
            inbound_session_id,
            peer_id,
            protocol_name: PROTOCOL_NAME.clone(),
        }),
    );
}

fn simulate_received_response(
    behaviour: &mut Behaviour,
    peer_id: PeerId,
    response: Bytes,
    outbound_session_id: OutboundSessionId,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        // This is the same connection_id from simulate_peer_assigned
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::GenerateEvent(GenericEvent::ReceivedResponse {
            response,
            outbound_session_id,
            peer_id,
        }),
    );
}

fn simulate_session_finished_successfully(
    behaviour: &mut Behaviour,
    peer_id: PeerId,
    session_id: SessionId,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        // This is the same connection_id from simulate_peer_assigned
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFinishedSuccessfully {
            session_id,
        }),
    );
}

fn simulate_connection_closed(behaviour: &mut Behaviour, peer_id: PeerId) {
    // This is the same connection_id from simulate_peer_assigned
    let connection_id = ConnectionId::new_unchecked(0);
    behaviour.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
        peer_id,
        connection_id,
        // Filling these fields with arbitrary values since the behaviour doesn't look at these
        // fields.
        endpoint: &ConnectedPoint::Dialer {
            address: Multiaddr::empty(),
            role_override: Endpoint::Dialer,
        },
        remaining_established: 0,
    }))
}

fn simulate_session_dropped(behaviour: &mut Behaviour, peer_id: PeerId, session_id: SessionId) {
    behaviour.on_connection_handler_event(
        peer_id,
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::NotifySessionDropped { session_id },
    );
}

async fn validate_request_peer_assignment_event(
    behaviour: &mut Behaviour,
    outbound_session_id: OutboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::ToOtherBehaviourEvent(ToOtherBehaviourEvent::RequestPeerAssignment {
                outbound_session_id: event_outbound_session_id
            },
        )) if outbound_session_id == event_outbound_session_id
    );
}

async fn validate_create_outbound_session_event(
    behaviour: &mut Behaviour,
    peer_id: &PeerId,
    query: &Bytes,
    outbound_session_id: &OutboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::CreateOutboundSession { query: event_query, outbound_session_id: event_outbound_session_id, protocol_name },
            ..
        } if *peer_id == event_peer_id
            && *outbound_session_id == event_outbound_session_id
            && *query == event_query
            && protocol_name == PROTOCOL_NAME.clone()
    );
}

async fn validate_new_inbound_session_event(
    behaviour: &mut Behaviour,
    peer_id: &PeerId,
    inbound_session_id: InboundSessionId,
    query: &Bytes,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::External(ExternalEvent::NewInboundSession {
            query: event_query,
            inbound_session_id: event_inbound_session_id,
            peer_id: event_peer_id,
            protocol_name,
        })) if event_query == *query
            && event_inbound_session_id == inbound_session_id
            && event_peer_id == *peer_id
            && protocol_name == PROTOCOL_NAME.clone()
    );
}

async fn validate_received_response_event(
    behaviour: &mut Behaviour,
    response: &Bytes,
    outbound_session_id: OutboundSessionId,
    peer_id: PeerId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::External(ExternalEvent::ReceivedResponse {
            response: event_response, outbound_session_id: event_outbound_session_id,
            peer_id: event_peer_id,
        })) if event_response == *response && event_outbound_session_id == outbound_session_id && peer_id == event_peer_id
    );
}

async fn validate_request_send_response_event(
    behaviour: &mut Behaviour,
    peer_id: &PeerId,
    response: &Bytes,
    inbound_session_id: InboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::SendResponse {
                inbound_session_id: event_inbound_session_id, response: event_response
            },
            ..
        } if *peer_id == event_peer_id
            && inbound_session_id == event_inbound_session_id
            && *response == event_response
    );
}

async fn validate_request_close_inbound_session_event(
    behaviour: &mut Behaviour,
    peer_id: &PeerId,
    inbound_session_id: InboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::CloseInboundSession {
                inbound_session_id: event_inbound_session_id
            },
            ..
        } if *peer_id == event_peer_id
            && inbound_session_id == event_inbound_session_id
    );
}

async fn validate_request_drop_session_event(
    behaviour: &mut Behaviour,
    peer_id: &PeerId,
    session_id: SessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::DropSession {
                session_id: event_session_id
            },
            ..
        } if *peer_id == event_peer_id && session_id == event_session_id
    );
}

async fn validate_session_finished_successfully_event(
    behaviour: &mut Behaviour,
    session_id: SessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::External(ExternalEvent::SessionFinishedSuccessfully {
            session_id: event_session_id
        })) if event_session_id == session_id
    );
}

// TODO(shahak): Fix code duplication with handler test.
fn validate_no_events(behaviour: &mut Behaviour) {
    assert!(behaviour.next().now_or_never().is_none());
}

#[tokio::test]
async fn process_inbound_session() {
    let mut behaviour = Behaviour::new(Config::get_test_config());

    let peer_id = PeerId::random();
    let inbound_session_id = InboundSessionId::default();

    simulate_new_inbound_session(&mut behaviour, peer_id, inbound_session_id, QUERY.clone());
    validate_new_inbound_session_event(&mut behaviour, &peer_id, inbound_session_id, &QUERY).await;
    validate_no_events(&mut behaviour);

    let dummy_data_vec = dummy_data();
    for response in &dummy_data_vec {
        behaviour.send_response(response.clone(), inbound_session_id).unwrap();
    }

    for response in &dummy_data_vec {
        validate_request_send_response_event(
            &mut behaviour,
            &peer_id,
            response,
            inbound_session_id,
        )
        .await;
    }
    validate_no_events(&mut behaviour);

    behaviour.close_inbound_session(inbound_session_id).unwrap();
    validate_request_close_inbound_session_event(&mut behaviour, &peer_id, inbound_session_id)
        .await;
    validate_no_events(&mut behaviour);

    let session_id = inbound_session_id.into();
    simulate_session_finished_successfully(&mut behaviour, peer_id, session_id);
    validate_session_finished_successfully_event(&mut behaviour, session_id).await;
    validate_no_events(&mut behaviour);
}

#[tokio::test]
async fn create_and_process_outbound_session() {
    let mut behaviour = Behaviour::new(Config::get_test_config());

    let peer_id = PeerId::random();

    let outbound_session_id = behaviour.start_query(QUERY.clone(), PROTOCOL_NAME.clone());

    validate_request_peer_assignment_event(&mut behaviour, outbound_session_id).await;
    validate_no_events(&mut behaviour);
    simulate_peer_assigned(&mut behaviour, peer_id, outbound_session_id);

    validate_create_outbound_session_event(&mut behaviour, &peer_id, &QUERY, &outbound_session_id)
        .await;
    validate_no_events(&mut behaviour);

    let dummy_data_vec = dummy_data();
    for response in &dummy_data_vec {
        simulate_received_response(&mut behaviour, peer_id, response.clone(), outbound_session_id);
    }

    for response in &dummy_data_vec {
        validate_received_response_event(&mut behaviour, response, outbound_session_id, peer_id)
            .await;
    }
    validate_no_events(&mut behaviour);

    let session_id = outbound_session_id.into();
    simulate_session_finished_successfully(&mut behaviour, peer_id, session_id);
    validate_session_finished_successfully_event(&mut behaviour, session_id).await;
    validate_no_events(&mut behaviour);
}

// TODO(shahak): Test the other variants of SessionError.
#[tokio::test]
async fn connection_closed() {
    let mut behaviour = Behaviour::new(Config::get_test_config());

    let peer_id = PeerId::random();

    // Add an outbound session on the connection.
    let outbound_session_id = behaviour.start_query(QUERY.clone(), PROTOCOL_NAME.clone());
    // Consume the event to request peer assignment.
    behaviour.next().await.unwrap();
    simulate_peer_assigned(&mut behaviour, peer_id, outbound_session_id);
    // Consume the event to create an outbound session.
    behaviour.next().await.unwrap();

    // Add an inbound session on the connection.
    let inbound_session_id = InboundSessionId::default();
    simulate_new_inbound_session(&mut behaviour, peer_id, inbound_session_id, QUERY.clone());
    // Consume the event to notify the user about the new inbound session.
    behaviour.next().await.unwrap();

    simulate_connection_closed(&mut behaviour, peer_id);

    let event1 = behaviour.next().await.unwrap();
    let event2 = behaviour.next().await.unwrap();
    let failed_session_ids = [event1, event2]
        .iter()
        .map(|event| {
            let ToSwarm::GenerateEvent(Event::External(ExternalEvent::SessionFailed {
                error: SessionError::ConnectionClosed,
                session_id,
            })) = event
            else {
                panic!(
                    "Event {:?} doesn't match expected event \
                     ToSwarm::GenerateEvent(Event::External(ExternalEvent::SessionFailed {{ \
                     error: SessionError::ConnectionClosed }}))",
                    event
                );
            };
            *session_id
        })
        .collect::<Vec<_>>();
    assert!(
        failed_session_ids == vec![inbound_session_id.into(), outbound_session_id.into()]
            || failed_session_ids == vec![outbound_session_id.into(), inbound_session_id.into()]
    );
}

#[tokio::test]
async fn drop_outbound_session() {
    let mut behaviour = Behaviour::new(Config::get_test_config());

    let peer_id = PeerId::random();

    let outbound_session_id = behaviour.start_query(QUERY.clone(), PROTOCOL_NAME.clone());
    // Consume the event to request peer assignment.
    behaviour.next().await.unwrap();
    simulate_peer_assigned(&mut behaviour, peer_id, outbound_session_id);
    // Consume the event to create an outbound session.
    behaviour.next().await.unwrap();

    behaviour.drop_session(outbound_session_id.into()).unwrap();
    validate_request_drop_session_event(&mut behaviour, &peer_id, outbound_session_id.into()).await;

    for response in dummy_data() {
        simulate_received_response(&mut behaviour, peer_id, response, outbound_session_id);
    }

    validate_no_events(&mut behaviour);

    simulate_session_finished_successfully(&mut behaviour, peer_id, outbound_session_id.into());

    validate_no_events(&mut behaviour);

    simulate_session_dropped(&mut behaviour, peer_id, outbound_session_id.into());

    // After this event the handler should not send any events to the behaviour about this session,
    // so if it will the behaviour might output them.
}

#[tokio::test]
async fn drop_inbound_session() {
    let mut behaviour = Behaviour::new(Config::get_test_config());

    let peer_id = PeerId::random();
    let inbound_session_id = InboundSessionId::default();

    simulate_new_inbound_session(&mut behaviour, peer_id, inbound_session_id, QUERY.clone());

    // Consume the event that a new inbound session was created.
    behaviour.next().await.unwrap();

    behaviour.drop_session(inbound_session_id.into()).unwrap();
    validate_request_drop_session_event(&mut behaviour, &peer_id, inbound_session_id.into()).await;

    simulate_session_finished_successfully(&mut behaviour, peer_id, inbound_session_id.into());

    validate_no_events(&mut behaviour);

    simulate_session_dropped(&mut behaviour, peer_id, inbound_session_id.into());

    // After this event the handler should not send any events to the behaviour about this session,
    // so if it will the behaviour might output them.
}

#[test]
fn close_non_existing_session_fails() {
    let mut behaviour = Behaviour::new(Config::get_test_config());
    behaviour.close_inbound_session(InboundSessionId::default()).unwrap_err();
}

#[test]
fn send_response_non_existing_session_fails() {
    let mut behaviour = Behaviour::new(Config::get_test_config());
    for response in dummy_data() {
        behaviour.send_response(response, InboundSessionId::default()).unwrap_err();
    }
}
