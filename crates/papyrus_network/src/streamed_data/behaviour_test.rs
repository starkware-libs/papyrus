use std::pin::Pin;
use std::task::{Context, Poll};

use assert_matches::assert_matches;
use futures::{FutureExt, Stream, StreamExt};
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{ConnectionClosed, ConnectionId, FromSwarm, NetworkBehaviour, ToSwarm};
use libp2p::{Multiaddr, PeerId};

use super::super::handler::{RequestFromBehaviourEvent, RequestToBehaviourEvent};
use super::super::{
    Config,
    DataBound,
    GenericEvent,
    InboundSessionId,
    OutboundSessionId,
    QueryBound,
    SessionId,
};
use super::{Behaviour, Event, SessionError};
use crate::messages::protobuf;
use crate::test_utils::dummy_data;
use crate::PapyrusBehaviour;

impl<Query: QueryBound, Data: DataBound> Unpin for Behaviour<Query, Data> {}

impl<Query: QueryBound, Data: DataBound> Stream for Behaviour<Query, Data> {
    type Item = ToSwarm<Event<Query, Data>, RequestFromBehaviourEvent<Query, Data>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

fn simulate_connection_established<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
) {
    let connection_id = ConnectionId::new_unchecked(0);
    let address = Multiaddr::empty();
    let role_override = Endpoint::Dialer;
    let _handler = behaviour
        .handle_established_outbound_connection(connection_id, peer_id, &address, role_override)
        .unwrap();
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id,
        endpoint: &ConnectedPoint::Dialer { address, role_override },
        failed_addresses: &[],
        other_established: 0,
    }));
}

fn simulate_listener_connection<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
) {
    let connection_id = ConnectionId::new_unchecked(0);
    let address = Multiaddr::empty();
    let local_addr = Multiaddr::empty();
    let role_override = Endpoint::Listener;
    let _handler = behaviour
        .handle_established_outbound_connection(connection_id, peer_id, &address, role_override)
        .unwrap();
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id,
        connection_id,
        endpoint: &ConnectedPoint::Listener { send_back_addr: address, local_addr },
        failed_addresses: &[],
        other_established: 0,
    }));
}

fn simulate_new_inbound_session<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
    inbound_session_id: InboundSessionId,
    query: Query,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::GenerateEvent(GenericEvent::NewInboundSession {
            query,
            inbound_session_id,
            peer_id,
        }),
    );
}

fn simulate_received_data<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
    data: Data,
    outbound_session_id: OutboundSessionId,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::GenerateEvent(GenericEvent::ReceivedData {
            data,
            outbound_session_id,
        }),
    );
}

fn simulate_session_finished_successfully<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
    session_id: SessionId,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFinishedSuccessfully {
            session_id,
        }),
    );
}

fn simulate_connection_closed<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
) {
    // This is the same connection_id from simulate_connection_established
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

fn simulate_outbound_session_dropped<Query: QueryBound, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: PeerId,
    outbound_session_id: OutboundSessionId,
) {
    behaviour.on_connection_handler_event(
        peer_id,
        ConnectionId::new_unchecked(0),
        RequestToBehaviourEvent::NotifyOutboundSessionDropped { outbound_session_id },
    );
}

async fn validate_create_outbound_session_event<Query: QueryBound + PartialEq, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
    query: &Query,
    outbound_session_id: &OutboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::CreateOutboundSession { query: event_query, outbound_session_id: event_outbound_session_id },
            ..
        } if *peer_id == event_peer_id
            && *outbound_session_id == event_outbound_session_id
            && *query == event_query
    );
}

async fn validate_new_inbound_session_event<Query: QueryBound + PartialEq, Data: DataBound>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
    inbound_session_id: InboundSessionId,
    query: &Query,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::NewInboundSession {
            query: event_query,
            inbound_session_id: event_inbound_session_id,
            peer_id: event_peer_id,
        }) if event_query == *query
            && event_inbound_session_id == inbound_session_id
            && event_peer_id == *peer_id
    );
}

async fn validate_received_data_event<Query: QueryBound, Data: DataBound + PartialEq>(
    behaviour: &mut Behaviour<Query, Data>,
    data: &Data,
    outbound_session_id: OutboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::ReceivedData {
            data: event_data, outbound_session_id: event_outbound_session_id
        }) if event_data == *data && event_outbound_session_id == outbound_session_id
    );
}

async fn validate_request_send_data_event<Query: QueryBound, Data: DataBound + PartialEq>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
    data: &Data,
    inbound_session_id: InboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::SendData {
                inbound_session_id: event_inbound_session_id, data: event_data
            },
            ..
        } if *peer_id == event_peer_id
            && inbound_session_id == event_inbound_session_id
            && *data == event_data
    );
}

async fn validate_request_close_inbound_session_event<
    Query: QueryBound,
    Data: DataBound + PartialEq,
>(
    behaviour: &mut Behaviour<Query, Data>,
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

async fn validate_request_drop_outbound_session_event<
    Query: QueryBound,
    Data: DataBound + PartialEq,
>(
    behaviour: &mut Behaviour<Query, Data>,
    peer_id: &PeerId,
    outbound_session_id: OutboundSessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::NotifyHandler {
            peer_id: event_peer_id,
            event: RequestFromBehaviourEvent::DropOutboundSession {
                outbound_session_id: event_outbound_session_id
            },
            ..
        } if *peer_id == event_peer_id
            && outbound_session_id == event_outbound_session_id
    );
}

async fn validate_session_finished_successfully_event<
    Query: QueryBound,
    Data: DataBound + PartialEq,
>(
    behaviour: &mut Behaviour<Query, Data>,
    session_id: SessionId,
) {
    let event = behaviour.next().await.unwrap();
    assert_matches!(
        event,
        ToSwarm::GenerateEvent(Event::SessionFinishedSuccessfully {
            session_id: event_session_id
        }) if event_session_id == session_id
    );
}

// TODO(shahak): Fix code duplication with handler test.
fn validate_no_events<Query: QueryBound, Data: DataBound>(behaviour: &mut Behaviour<Query, Data>) {
    assert!(behaviour.next().now_or_never().is_none());
}

#[tokio::test]
async fn process_inbound_session() {
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());

    let query = protobuf::BasicMessage::default();
    let peer_id = PeerId::random();
    let inbound_session_id = InboundSessionId::default();

    simulate_listener_connection(&mut behaviour, peer_id);

    simulate_new_inbound_session(&mut behaviour, peer_id, inbound_session_id, query.clone());
    validate_new_inbound_session_event(&mut behaviour, &peer_id, inbound_session_id, &query).await;
    validate_no_events(&mut behaviour);

    let dummy_data_vec = dummy_data();
    for data in &dummy_data_vec {
        behaviour.send_data(data.clone(), inbound_session_id).unwrap();
    }

    for data in &dummy_data_vec {
        validate_request_send_data_event(&mut behaviour, &peer_id, data, inbound_session_id).await;
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
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());

    let query = protobuf::BasicMessage::default();
    let peer_id = PeerId::random();

    simulate_connection_established(&mut behaviour, peer_id);
    let outbound_session_id = behaviour.send_query(query.clone(), peer_id).unwrap();

    validate_create_outbound_session_event(&mut behaviour, &peer_id, &query, &outbound_session_id)
        .await;
    validate_no_events(&mut behaviour);

    let dummy_data_vec = dummy_data();
    for data in &dummy_data_vec {
        simulate_received_data(&mut behaviour, peer_id, data.clone(), outbound_session_id);
    }

    for data in &dummy_data_vec {
        validate_received_data_event(&mut behaviour, data, outbound_session_id).await;
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
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());

    let peer_id = PeerId::random();

    simulate_connection_established(&mut behaviour, peer_id);

    let query = protobuf::BasicMessage::default();
    let outbound_session_id = behaviour.send_query(query.clone(), peer_id).unwrap();

    // Consume the event to create an outbound session.
    behaviour.next().await.unwrap();

    let inbound_session_id = InboundSessionId::default();
    simulate_new_inbound_session(&mut behaviour, peer_id, inbound_session_id, query.clone());

    // Consume the event to notify the user about the new inbound session.
    behaviour.next().await.unwrap();

    simulate_connection_closed(&mut behaviour, peer_id);

    let event1 = behaviour.next().await.unwrap();
    let event2 = behaviour.next().await.unwrap();
    let failed_session_ids = [event1, event2]
        .iter()
        .map(|event| {
            let ToSwarm::GenerateEvent(Event::SessionFailed {
                error: SessionError::ConnectionClosed,
                session_id,
            }) = event
            else {
                panic!(
                    "Event {:?} doesn't match expected event \
                     ToSwarm::GenerateEvent(Event::SessionFailed {{ error: \
                     SessionError::ConnectionClosed }}",
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
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());

    let peer_id = PeerId::random();

    simulate_connection_established(&mut behaviour, peer_id);

    let query = protobuf::BasicMessage::default();
    let outbound_session_id = behaviour.send_query(query.clone(), peer_id).unwrap();

    // Consume the event to create an outbound session.
    behaviour.next().await.unwrap();

    behaviour.drop_outbound_session(outbound_session_id).unwrap();
    validate_request_drop_outbound_session_event(&mut behaviour, &peer_id, outbound_session_id)
        .await;

    for data in dummy_data() {
        simulate_received_data(&mut behaviour, peer_id, data, outbound_session_id);
    }

    validate_no_events(&mut behaviour);

    simulate_session_finished_successfully(&mut behaviour, peer_id, outbound_session_id.into());

    validate_no_events(&mut behaviour);

    simulate_outbound_session_dropped(&mut behaviour, peer_id, outbound_session_id);

    // After this event the handler should not send any events to the behaviour about this session,
    // so if it will the behaviour might output them.
}

#[test]
fn close_non_existing_session_fails() {
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());
    behaviour.close_inbound_session(InboundSessionId::default()).unwrap_err();
}

#[test]
fn send_data_non_existing_session_fails() {
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());
    for data in dummy_data() {
        behaviour.send_data(data, InboundSessionId::default()).unwrap_err();
    }
}

#[test]
fn send_query_peer_not_connected_fails() {
    let mut behaviour =
        Behaviour::<protobuf::BasicMessage, protobuf::BasicMessage>::new(Config::get_test_config());

    let query = protobuf::BasicMessage::default();
    let peer_id = PeerId::random();

    behaviour.send_query(query.clone(), peer_id).unwrap_err();
}
