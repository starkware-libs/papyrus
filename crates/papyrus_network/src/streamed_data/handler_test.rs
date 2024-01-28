use std::collections::HashSet;
use std::io;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use assert_matches::assert_matches;
use futures::task::{Context, Poll};
use futures::{select, AsyncReadExt, AsyncWriteExt, FutureExt, Stream as StreamTrait, StreamExt};
use libp2p::swarm::handler::{
    ConnectionEvent,
    DialUpgradeError,
    FullyNegotiatedInbound,
    FullyNegotiatedOutbound,
};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, Stream, StreamUpgradeError};
use libp2p::PeerId;

use super::super::{Config, DataBound, InboundSessionId, OutboundSessionId, QueryBound, SessionId};
use super::{Handler, HandlerEvent, RequestFromBehaviourEvent, SessionError, ToBehaviourEvent};
use crate::messages::{protobuf, read_message, write_message};
use crate::test_utils::{dummy_data, get_connected_streams};

impl<Query: QueryBound, Data: DataBound> Unpin for Handler<Query, Data> {}

impl<Query: QueryBound, Data: DataBound> StreamTrait for Handler<Query, Data> {
    type Item = HandlerEvent<Handler<Query, Data>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

fn simulate_request_to_send_data_from_swarm<Query: QueryBound, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    data: Data,
    inbound_session_id: InboundSessionId,
) {
    handler.on_behaviour_event(RequestFromBehaviourEvent::SendData { data, inbound_session_id });
}

fn simulate_request_to_send_query_from_swarm<Query: QueryBound, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    query: Query,
    outbound_session_id: OutboundSessionId,
) {
    handler.on_behaviour_event(RequestFromBehaviourEvent::CreateOutboundSession {
        query,
        outbound_session_id,
    });
}

fn simulate_request_to_close_session<Query: QueryBound, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    session_id: SessionId,
) {
    handler.on_behaviour_event(RequestFromBehaviourEvent::CloseSession { session_id });
}

fn simulate_negotiated_inbound_session_from_swarm<Query: QueryBound, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    query: Query,
    inbound_stream: Stream,
    inbound_session_id: InboundSessionId,
) {
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
        protocol: (query, inbound_stream),
        info: inbound_session_id,
    }));
}

fn simulate_negotiated_outbound_session_from_swarm<Query: QueryBound, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    outbound_stream: Stream,
    outbound_session_id: OutboundSessionId,
) {
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
        FullyNegotiatedOutbound { protocol: outbound_stream, info: outbound_session_id },
    ));
}

fn simulate_outbound_negotiation_failed<Query: QueryBound + PartialEq, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    outbound_session_id: OutboundSessionId,
    error: StreamUpgradeError<io::Error>,
) {
    handler.on_connection_event(ConnectionEvent::DialUpgradeError(DialUpgradeError {
        info: outbound_session_id,
        error,
    }));
}

async fn validate_new_inbound_session_event<Query: QueryBound + PartialEq, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    query: &Query,
    inbound_session_id: InboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::NewInboundSession {
            query: event_query,
            inbound_session_id: event_inbound_session_id,
            peer_id: event_peer_id,
        }) if event_query == *query
            && event_inbound_session_id == inbound_session_id
            && event_peer_id == handler.peer_id => {}
    );
}

async fn validate_received_data_event<Query: QueryBound, Data: DataBound + PartialEq>(
    handler: &mut Handler<Query, Data>,
    data: &Data,
    outbound_session_id: OutboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::ReceivedData {
            data: event_data, outbound_session_id: event_outbound_session_id
        }) if event_data == *data &&  event_outbound_session_id == outbound_session_id
    );
}

async fn validate_session_closed_by_request_event<
    Query: QueryBound,
    Data: DataBound + PartialEq,
>(
    handler: &mut Handler<Query, Data>,
    session_id: SessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::SessionClosedByRequest {
            session_id: event_session_id
        }) if event_session_id == session_id
    );
}

async fn validate_session_closed_by_peer_event<Query: QueryBound, Data: DataBound + PartialEq>(
    handler: &mut Handler<Query, Data>,
    session_id: SessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::SessionClosedByPeer {
            session_id: event_session_id
        }) if event_session_id == session_id
    );
}

async fn validate_session_failed_event<Query: QueryBound, Data: DataBound + PartialEq>(
    handler: &mut Handler<Query, Data>,
    session_id: SessionId,
    session_error_matcher: impl FnOnce(&SessionError) -> bool,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::SessionFailed {
            session_id: event_session_id,
            error,
        }) if event_session_id == session_id && session_error_matcher(&error)
    );
}

fn validate_no_events<Query: QueryBound, Data: DataBound>(handler: &mut Handler<Query, Data>) {
    assert!(handler.next().now_or_never().is_none());
}

async fn validate_request_to_swarm_new_outbound_session_to_swarm_event<
    Query: QueryBound + PartialEq,
    Data: DataBound,
>(
    handler: &mut Handler<Query, Data>,
    query: &Query,
    outbound_session_id: OutboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::OutboundSubstreamRequest{ protocol }
        if protocol.upgrade().query == *query && *protocol.info() == outbound_session_id
    );
}

async fn read_messages<Query: QueryBound, Data: DataBound>(
    handler: Handler<Query, Data>,
    stream: &mut Stream,
    num_messages: usize,
) -> Vec<protobuf::BasicMessage> {
    async fn read_messages_inner(
        stream: &mut Stream,
        num_messages: usize,
    ) -> Vec<protobuf::BasicMessage> {
        let mut result = Vec::new();
        for _ in 0..num_messages {
            match read_message::<protobuf::BasicMessage, _>(&mut *stream).await.unwrap() {
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
    let mut handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
        Config::get_test_config(),
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let query = protobuf::BasicMessage::default();
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        query.clone(),
        inbound_stream,
        inbound_session_id,
    );
    validate_new_inbound_session_event(&mut handler, &query, inbound_session_id).await;
    let dummy_data_vec = dummy_data();
    for data in &dummy_data_vec {
        simulate_request_to_send_data_from_swarm(&mut handler, data.clone(), inbound_session_id);
    }

    let data_received = read_messages(handler, &mut outbound_stream, dummy_data_vec.len()).await;
    assert_eq!(dummy_data_vec, data_received);
}

#[tokio::test]
async fn closed_inbound_session_ignores_behaviour_request_to_send_data() {
    let mut handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
        Config::get_test_config(),
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let query = protobuf::BasicMessage::default();
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        query.clone(),
        inbound_stream,
        inbound_session_id,
    );

    // consume the new inbound session event without reading it.
    handler.next().await;

    simulate_request_to_close_session(&mut handler, inbound_session_id.into());
    validate_session_closed_by_request_event(&mut handler, inbound_session_id.into()).await;

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
            let handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
                Config::get_test_config(),
                next_inbound_session_id,
                PeerId::random(),
            );
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
    let mut handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
        Config::get_test_config(),
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let query = protobuf::BasicMessage::default();
    let outbound_session_id = OutboundSessionId { value: 1 };

    simulate_request_to_send_query_from_swarm(&mut handler, query.clone(), outbound_session_id);
    validate_request_to_swarm_new_outbound_session_to_swarm_event(
        &mut handler,
        &query,
        outbound_session_id,
    )
    .await;

    simulate_negotiated_outbound_session_from_swarm(
        &mut handler,
        outbound_stream,
        outbound_session_id,
    );

    let dummy_data_vec = dummy_data();
    for data in dummy_data_vec.clone() {
        write_message(data, &mut inbound_stream).await.unwrap();
    }

    for data in &dummy_data_vec {
        validate_received_data_event(&mut handler, data, outbound_session_id).await;
    }

    validate_no_events(&mut handler);

    inbound_stream.close().await.unwrap();
    validate_session_closed_by_peer_event(&mut handler, outbound_session_id.into()).await;
}

// Extracting to a function because two closures have different types.
async fn test_outbound_session_negotiation_failure(
    upgrade_error: StreamUpgradeError<io::Error>,
    session_error_matcher: impl FnOnce(&SessionError) -> bool,
    config: Config,
) {
    let outbound_session_id = OutboundSessionId { value: 1 };
    let mut handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
        config,
        Arc::new(Default::default()),
        PeerId::random(),
    );
    simulate_outbound_negotiation_failed(&mut handler, outbound_session_id, upgrade_error);
    validate_session_failed_event(&mut handler, outbound_session_id.into(), session_error_matcher)
        .await;
    validate_no_events(&mut handler);
}

#[tokio::test]
async fn close_outbound_session() {
    let mut handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
        Config::get_test_config(),
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let query = protobuf::BasicMessage::default();
    let outbound_session_id = OutboundSessionId { value: 1 };

    simulate_request_to_send_query_from_swarm(&mut handler, query.clone(), outbound_session_id);

    // consume the event to request a new session from the swarm.
    handler.next().await;

    simulate_negotiated_outbound_session_from_swarm(
        &mut handler,
        outbound_stream,
        outbound_session_id,
    );

    simulate_request_to_close_session(&mut handler, outbound_session_id.into());

    // This should happen before checking that the session was closed on the inbound side in order
    // to poll the handler and then the handler will close the session.
    validate_session_closed_by_request_event(&mut handler, outbound_session_id.into()).await;

    // Check that outbound_stream was closed by reading and seeing we get 0 bytes back.
    let mut buffer = [0u8];
    assert_eq!(inbound_stream.read(&mut buffer).await.unwrap(), 0);
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
                SessionError::Timeout { substream_timeout }
                if *substream_timeout == config.substream_timeout
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
        |session_error| {
            matches!(
                session_error,
                SessionError::RemoteDoesntSupportProtocol { protocol_name }
                if *protocol_name == config.protocol_name
            )
        },
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
async fn closed_outbound_session_doesnt_emit_events_when_data_is_sent() {
    let mut handler = Handler::<protobuf::BasicMessage, protobuf::BasicMessage>::new(
        Config::get_test_config(),
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    let outbound_session_id = OutboundSessionId { value: 1 };

    simulate_negotiated_outbound_session_from_swarm(
        &mut handler,
        outbound_stream,
        outbound_session_id,
    );

    simulate_request_to_close_session(&mut handler, outbound_session_id.into());
    validate_session_closed_by_request_event(&mut handler, outbound_session_id.into()).await;

    for data in dummy_data() {
        // The handler might have already closed outbound_stream, so we don't unwrap the result
        let _ = write_message(data, &mut inbound_stream).await;
    }

    validate_no_events(&mut handler);
}
