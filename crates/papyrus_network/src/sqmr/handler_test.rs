use std::collections::HashSet;
use std::io;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use assert_matches::assert_matches;
use futures::task::{Context, Poll};
use futures::{select, AsyncReadExt, AsyncWriteExt, FutureExt, Stream as StreamTrait, StreamExt};
use lazy_static::lazy_static;
use libp2p::swarm::handler::{
    ConnectionEvent,
    DialUpgradeError,
    FullyNegotiatedInbound,
    FullyNegotiatedOutbound,
};
use libp2p::swarm::{
    ConnectionHandler,
    ConnectionHandlerEvent,
    Stream,
    StreamProtocol,
    StreamUpgradeError,
};
use libp2p::PeerId;

use super::super::messages::{read_message, write_message};
use super::super::{Bytes, Config, GenericEvent, InboundSessionId, OutboundSessionId, SessionId};
use super::{
    Handler,
    HandlerEvent,
    RequestFromBehaviourEvent,
    RequestToBehaviourEvent,
    SessionError,
};
use crate::test_utils::{dummy_data, get_connected_streams};

impl Unpin for Handler {}

impl StreamTrait for Handler {
    type Item = HandlerEvent<Handler>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

lazy_static! {
    static ref QUERY: Bytes = vec![1u8, 2u8, 3u8];
    static ref PROTOCOL_NAME: StreamProtocol =
        Config::get_test_config().supported_inbound_protocols.first().unwrap().clone();
}

fn simulate_request_to_send_data_from_swarm(
    handler: &mut Handler,
    data: Bytes,
    inbound_session_id: InboundSessionId,
) {
    handler.on_behaviour_event(RequestFromBehaviourEvent::SendData { data, inbound_session_id });
}

fn simulate_request_to_send_query_from_swarm(
    handler: &mut Handler,
    query: Bytes,
    outbound_session_id: OutboundSessionId,
) {
    handler.on_behaviour_event(RequestFromBehaviourEvent::CreateOutboundSession {
        query,
        outbound_session_id,
        protocol_name: PROTOCOL_NAME.clone(),
    });
}

fn simulate_request_to_close_inbound_session(
    handler: &mut Handler,
    inbound_session_id: InboundSessionId,
) {
    handler
        .on_behaviour_event(RequestFromBehaviourEvent::CloseInboundSession { inbound_session_id });
}

fn simulate_request_to_drop_session(handler: &mut Handler, session_id: SessionId) {
    handler.on_behaviour_event(RequestFromBehaviourEvent::DropSession { session_id });
}

fn simulate_negotiated_inbound_session_from_swarm(
    handler: &mut Handler,
    query: Bytes,
    inbound_stream: Stream,
    inbound_session_id: InboundSessionId,
) {
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
        protocol: (query, inbound_stream.split().1, PROTOCOL_NAME.clone()),
        info: inbound_session_id,
    }));
}

fn simulate_negotiated_outbound_session_from_swarm(
    handler: &mut Handler,
    outbound_stream: Stream,
    outbound_session_id: OutboundSessionId,
) {
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
        FullyNegotiatedOutbound { protocol: outbound_stream.split().0, info: outbound_session_id },
    ));
}

fn simulate_outbound_negotiation_failed(
    handler: &mut Handler,
    outbound_session_id: OutboundSessionId,
    error: StreamUpgradeError<io::Error>,
) {
    handler.on_connection_event(ConnectionEvent::DialUpgradeError(DialUpgradeError {
        info: outbound_session_id,
        error,
    }));
}

async fn validate_new_inbound_session_event(
    handler: &mut Handler,
    query: &Bytes,
    inbound_session_id: InboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(
            RequestToBehaviourEvent::GenerateEvent(
                GenericEvent::NewInboundSession {
                    query: event_query,
                    inbound_session_id: event_inbound_session_id,
                    peer_id: event_peer_id,
                    protocol_name,
                }
            )
        ) if event_query == *query
            && event_inbound_session_id == inbound_session_id
            && event_peer_id == handler.peer_id
            && protocol_name == PROTOCOL_NAME.clone() => {}
    );
}

async fn validate_received_data_event(
    handler: &mut Handler,
    data: &Bytes,
    outbound_session_id: OutboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(
            RequestToBehaviourEvent::GenerateEvent(
                GenericEvent::ReceivedData {
                    data: event_data, outbound_session_id: event_outbound_session_id

                }
            )
        ) if event_data == *data &&  event_outbound_session_id == outbound_session_id
    );
}

async fn validate_session_finished_successfully_event(
    handler: &mut Handler,
    session_id: SessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFinishedSuccessfully {
            session_id: event_session_id
        })) if event_session_id == session_id
    );
}

async fn validate_session_failed_event(
    handler: &mut Handler,
    session_id: SessionId,
    session_error_matcher: impl FnOnce(&SessionError) -> bool,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(
            RequestToBehaviourEvent::GenerateEvent(GenericEvent::SessionFailed {
                session_id: event_session_id,
                error,
            })
        ) if event_session_id == session_id && session_error_matcher(&error)
    );
}

async fn validate_session_dropped_event(handler: &mut Handler, session_id: SessionId) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(
            RequestToBehaviourEvent::NotifySessionDropped {
                session_id: event_session_id
            }
        ) if event_session_id == session_id
    );
}

fn validate_no_events(handler: &mut Handler) {
    assert!(handler.next().now_or_never().is_none());
}

async fn validate_request_to_swarm_new_outbound_session_to_swarm_event(
    handler: &mut Handler,
    query: &Bytes,
    outbound_session_id: OutboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::OutboundSubstreamRequest{ protocol }
        if protocol.upgrade().query == *query && *protocol.info() == outbound_session_id
    );
}

async fn read_messages(handler: Handler, stream: &mut Stream, num_messages: usize) -> Vec<Bytes> {
    async fn read_messages_inner(stream: &mut Stream, num_messages: usize) -> Vec<Bytes> {
        let mut result = Vec::new();
        for _ in 0..num_messages {
            match read_message(&mut *stream).await.unwrap() {
                Some(message) => result.push(message),
                None => return result,
            }
        }
        result
    }

    let mut fused_handler = handler.fuse();
    select! {
        data = read_messages_inner(stream, num_messages).fuse() => data,
        _ = fused_handler.next() => panic!("There shouldn't be another event from the handler"),
    }
}

#[tokio::test]
async fn process_inbound_session() {
    let mut handler =
        Handler::new(Config::get_test_config(), Arc::new(Default::default()), PeerId::random());

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        QUERY.clone(),
        inbound_stream,
        inbound_session_id,
    );
    validate_new_inbound_session_event(&mut handler, &QUERY, inbound_session_id).await;
    let dummy_data_vec = dummy_data();
    for data in &dummy_data_vec {
        simulate_request_to_send_data_from_swarm(&mut handler, data.clone(), inbound_session_id);
    }

    let data_received = read_messages(handler, &mut outbound_stream, dummy_data_vec.len()).await;
    assert_eq!(dummy_data_vec, data_received);
}

#[tokio::test]
async fn closed_inbound_session_ignores_behaviour_request_to_send_data() {
    let mut handler =
        Handler::new(Config::get_test_config(), Arc::new(Default::default()), PeerId::random());

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        QUERY.clone(),
        inbound_stream,
        inbound_session_id,
    );

    // consume the new inbound session event without reading it.
    handler.next().await;

    simulate_request_to_close_inbound_session(&mut handler, inbound_session_id);
    validate_session_finished_successfully_event(&mut handler, inbound_session_id.into()).await;

    let dummy_data_vec = dummy_data();
    for data in &dummy_data_vec {
        simulate_request_to_send_data_from_swarm(&mut handler, data.clone(), inbound_session_id);
    }
    let data_received = read_messages(handler, &mut outbound_stream, 1).await;
    assert!(data_received.is_empty());
}

#[test]
fn listen_protocol_across_multiple_handlers() {
    let next_inbound_session_id = Arc::new(AtomicUsize::default());
    const NUM_HANDLERS: usize = 5;
    const NUM_PROTOCOLS_PER_HANDLER: usize = 10;
    let thread_handles = (0..NUM_HANDLERS).map(|_| {
        let next_inbound_session_id = next_inbound_session_id.clone();
        std::thread::spawn(|| {
            let handler =
                Handler::new(Config::get_test_config(), next_inbound_session_id, PeerId::random());
            (0..NUM_PROTOCOLS_PER_HANDLER)
                .map(|_| handler.listen_protocol().info().value)
                .collect::<Vec<_>>()
        })
    });
    let inbound_session_ids =
        thread_handles.flat_map(|handle| handle.join().unwrap()).collect::<HashSet<_>>();
    assert_eq!(
        (0..(NUM_HANDLERS * NUM_PROTOCOLS_PER_HANDLER)).collect::<HashSet<_>>(),
        inbound_session_ids
    );
}

#[tokio::test]
async fn process_outbound_session() {
    let mut handler =
        Handler::new(Config::get_test_config(), Arc::new(Default::default()), PeerId::random());

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let outbound_session_id = OutboundSessionId { value: 1 };

    simulate_request_to_send_query_from_swarm(&mut handler, QUERY.clone(), outbound_session_id);
    validate_request_to_swarm_new_outbound_session_to_swarm_event(
        &mut handler,
        &QUERY,
        outbound_session_id,
    )
    .await;

    simulate_negotiated_outbound_session_from_swarm(
        &mut handler,
        outbound_stream,
        outbound_session_id,
    );

    let dummy_data_vec = dummy_data();
    for data in &dummy_data_vec {
        write_message(data, &mut inbound_stream).await.unwrap();
    }

    for data in &dummy_data_vec {
        validate_received_data_event(&mut handler, data, outbound_session_id).await;
    }

    validate_no_events(&mut handler);

    inbound_stream.close().await.unwrap();
    validate_session_finished_successfully_event(&mut handler, outbound_session_id.into()).await;
}

// Extracting to a function because two closures have different types.
async fn test_outbound_session_negotiation_failure(
    upgrade_error: StreamUpgradeError<io::Error>,
    session_error_matcher: impl FnOnce(&SessionError) -> bool,
    config: Config,
) {
    let outbound_session_id = OutboundSessionId { value: 1 };
    let mut handler = Handler::new(config, Arc::new(Default::default()), PeerId::random());
    simulate_outbound_negotiation_failed(&mut handler, outbound_session_id, upgrade_error);
    validate_session_failed_event(&mut handler, outbound_session_id.into(), session_error_matcher)
        .await;
    validate_no_events(&mut handler);
}

// TODO(shahak): Add tests where session fails after negotiation.
#[tokio::test]
async fn outbound_session_negotiation_failure() {
    let error_kind = io::ErrorKind::UnexpectedEof;
    let config = Config::get_test_config();
    test_outbound_session_negotiation_failure(
        StreamUpgradeError::Timeout,
        |session_error| {
            matches!(
                session_error,
                SessionError::Timeout { session_timeout }
                if *session_timeout == config.session_timeout
            )
        },
        config.clone(),
    )
    .await;
    test_outbound_session_negotiation_failure(
        StreamUpgradeError::Apply(error_kind.into()),
        |session_error| {
            matches!(
                session_error,
                SessionError::IOError(error)
                if error.kind() == error_kind
            )
        },
        config.clone(),
    )
    .await;
    test_outbound_session_negotiation_failure(
        StreamUpgradeError::NegotiationFailed,
        |session_error| matches!(session_error, SessionError::RemoteDoesntSupportProtocol),
        config.clone(),
    )
    .await;
    test_outbound_session_negotiation_failure(
        StreamUpgradeError::Io(error_kind.into()),
        |session_error| {
            matches!(
                session_error,
                SessionError::IOError(error)
                if error.kind() == error_kind
            )
        },
        config.clone(),
    )
    .await;
}

#[tokio::test]
async fn outbound_session_dropped_after_negotiation() {
    let mut handler =
        Handler::new(Config::get_test_config(), Arc::new(Default::default()), PeerId::random());

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let outbound_session_id = OutboundSessionId { value: 1 };

    simulate_request_to_send_query_from_swarm(&mut handler, QUERY.clone(), outbound_session_id);
    // consume the new outbound session event without reading it.
    handler.next().await;

    simulate_negotiated_outbound_session_from_swarm(
        &mut handler,
        outbound_stream,
        outbound_session_id,
    );

    simulate_request_to_drop_session(&mut handler, outbound_session_id.into());
    validate_session_dropped_event(&mut handler, outbound_session_id.into()).await;

    // Need to sleep to make sure the dropping occurs on the other stream.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    write_message(dummy_data().first().unwrap(), &mut inbound_stream).await.unwrap_err();

    // Need to sleep to make sure that if we did send a message the stream inside the handle will
    // receive it
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn outbound_session_dropped_before_negotiation() {
    let mut handler =
        Handler::new(Config::get_test_config(), Arc::new(Default::default()), PeerId::random());

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let outbound_session_id = OutboundSessionId { value: 1 };

    simulate_request_to_send_query_from_swarm(&mut handler, QUERY.clone(), outbound_session_id);
    // consume the new outbound session event without reading it.
    handler.next().await;

    simulate_request_to_drop_session(&mut handler, outbound_session_id.into());
    validate_session_dropped_event(&mut handler, outbound_session_id.into()).await;

    simulate_negotiated_outbound_session_from_swarm(
        &mut handler,
        outbound_stream,
        outbound_session_id,
    );

    // Need to sleep to make sure the dropping occurs on the other stream.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    write_message(&dummy_data().first().unwrap().clone(), &mut inbound_stream).await.unwrap_err();

    // Need to sleep to make sure that if we did send a message the stream inside the handle will
    // receive it
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    validate_no_events(&mut handler);
}

#[tokio::test]
async fn inbound_session_dropped() {
    let mut handler =
        Handler::new(Config::get_test_config(), Arc::new(Default::default()), PeerId::random());

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        QUERY.clone(),
        inbound_stream,
        inbound_session_id,
    );
    // consume the new inbound session event without reading it.
    handler.next().await;

    simulate_request_to_drop_session(&mut handler, inbound_session_id.into());
    validate_session_dropped_event(&mut handler, inbound_session_id.into()).await;

    // Need to sleep to make sure the dropping occurs on the other stream.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // A dropped inbound session will return EOF.
    assert!(read_message(&mut outbound_stream).await.unwrap().is_none());

    // Need to sleep to make sure that if we did send a message the stream inside the handle will
    // receive it
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    validate_no_events(&mut handler);
}
