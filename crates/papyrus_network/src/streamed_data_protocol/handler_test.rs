use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use futures::task::{Context, Poll};
use futures::{select, AsyncWriteExt, FutureExt, Stream as StreamTrait, StreamExt};
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, Stream};
use libp2p::PeerId;

use super::super::{DataBound, InboundSessionId, OutboundSessionId, QueryBound, SessionId};
use super::{Handler, HandlerEvent, RequestFromBehaviourEvent, ToBehaviourEvent};
use crate::messages::{protobuf, read_message, write_message};
use crate::test_utils::{get_connected_streams, hardcoded_data};

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

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

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

async fn validate_outbound_session_closed_by_peer_event<
    Query: QueryBound,
    Data: DataBound + PartialEq,
>(
    handler: &mut Handler<Query, Data>,
    outbound_session_id: OutboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::OutboundSessionClosedByPeer {
            outbound_session_id: event_outbound_session_id
        }) if event_outbound_session_id == outbound_session_id
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
) -> Vec<protobuf::BlockHeadersResponse> {
    async fn read_messages_inner(
        stream: &mut Stream,
        num_messages: usize,
    ) -> Vec<protobuf::BlockHeadersResponse> {
        let mut result = Vec::new();
        for _ in 0..num_messages {
            match read_message::<protobuf::BlockHeadersResponse, _>(&mut *stream).await.unwrap() {
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
    let mut handler = Handler::<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>::new(
        SUBSTREAM_TIMEOUT,
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    // TODO(shahak): Change to GetBlocks::default() when the bug that forbids sending default
    // messages is fixed.
    let query = protobuf::BlockHeadersRequest { ..Default::default() };
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        query.clone(),
        inbound_stream,
        inbound_session_id,
    );
    validate_new_inbound_session_event(&mut handler, &query, inbound_session_id).await;
    let hardcoded_data_vec = hardcoded_data();
    for data in &hardcoded_data_vec {
        simulate_request_to_send_data_from_swarm(&mut handler, data.clone(), inbound_session_id);
    }

    let data_received =
        read_messages(handler, &mut outbound_stream, hardcoded_data_vec.len()).await;
    assert_eq!(hardcoded_data_vec, data_received);
}

#[tokio::test]
async fn closed_inbound_session_ignores_behaviour_request_to_send_data() {
    let mut handler = Handler::<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>::new(
        SUBSTREAM_TIMEOUT,
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    // TODO(shahak): Change to protobuf::BlockHeadersRequest::default() when the bug that forbids
    // sending default messages is fixed.
    let query = protobuf::BlockHeadersRequest { ..Default::default() };
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_negotiated_inbound_session_from_swarm(
        &mut handler,
        query.clone(),
        inbound_stream,
        inbound_session_id,
    );

    // consume the new inbound session event without reading it.
    handler.next().await;

    simulate_request_to_close_session(
        &mut handler,
        SessionId::InboundSessionId(inbound_session_id),
    );
    validate_session_closed_by_request_event(
        &mut handler,
        SessionId::InboundSessionId(inbound_session_id),
    )
    .await;

    let hardcoded_data_vec = hardcoded_data();
    for data in &hardcoded_data_vec {
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
                Handler::<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>::new(
                    SUBSTREAM_TIMEOUT,
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
    let mut handler = Handler::<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>::new(
        SUBSTREAM_TIMEOUT,
        Arc::new(Default::default()),
        PeerId::random(),
    );

    let (mut inbound_stream, outbound_stream, _) = get_connected_streams().await;
    // TODO(shahak): Change to protobuf::BlockHeadersRequest::default() when the bug that forbids
    // sending default messages is fixed.
    let query = protobuf::BlockHeadersRequest { ..Default::default() };
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

    let hardcoded_data_vec = hardcoded_data();
    for data in hardcoded_data_vec.clone() {
        write_message(data, &mut inbound_stream).await.unwrap();
    }

    for data in &hardcoded_data_vec {
        validate_received_data_event(&mut handler, data, outbound_session_id).await;
    }

    validate_no_events(&mut handler);

    inbound_stream.close().await.unwrap();
    validate_outbound_session_closed_by_peer_event(&mut handler, outbound_session_id).await;
}

#[tokio::test]
async fn closed_outbound_session_doesnt_emit_events_when_data_is_sent() {
    let mut handler = Handler::<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>::new(
        SUBSTREAM_TIMEOUT,
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

    simulate_request_to_close_session(
        &mut handler,
        SessionId::OutboundSessionId(outbound_session_id),
    );
    validate_session_closed_by_request_event(
        &mut handler,
        SessionId::OutboundSessionId(outbound_session_id),
    )
    .await;

    for data in hardcoded_data() {
        write_message(data, &mut inbound_stream).await.unwrap();
    }

    validate_no_events(&mut handler);
}
// async fn start_request_and_validate_event<
//     Query: Message + PartialEq + Clone,
//     Data: Message + Default,
// >(
//     handler: &mut Handler<Query, Data>,
//     query: &Query,
//     outbound_session_id: OutboundSessionId,
// ) -> UnboundedSender<Data> { handler.on_behaviour_event(NewQueryEvent { query: query.clone(),
//   outbound_session_id }); let event = handler.next().await.unwrap(); let
//   ConnectionHandlerEvent::OutboundSubstreamRequest { protocol } = event else { panic!("Got
//   unexpected event"); }; assert_eq!(*query, *protocol.upgrade().query());
//   assert_eq!(SUBSTREAM_TIMEOUT, *protocol.timeout()); protocol.upgrade().data_sender().clone()
// }

// async fn send_data_and_validate_event<
//     Query: Message,
//     Data: Message + Default + PartialEq + Clone,
// >(
//     handler: &mut Handler<Query, Data>,
//     data: &Data,
//     outbound_session_id: OutboundSessionId,
//     data_sender: &UnboundedSender<Data>,
// ) { data_sender.unbounded_send(data.clone()).unwrap(); let event = handler.next().await.unwrap();
//   assert_matches!( event,
//   ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::ReceivedData{
//   outbound_session_id: event_outbound_session_id, data: event_data }) if
//   event_outbound_session_id == outbound_session_id && event_data == *data );
// }

// async fn finish_session_and_validate_event<Query: Message, Data: Message + Default>(
//     handler: &mut Handler<Query, Data>,
//     outbound_session_id: OutboundSessionId,
// ) { handler.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound( FullyNegotiatedOutbound
//   { protocol: (), info: outbound_session_id }, )); let event = handler.next().await.unwrap();
//   assert_matches!( event,
//   ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::SessionFinished{
//   outbound_session_id: event_outbound_session_id }) if event_outbound_session_id ==
//   outbound_session_id );
// }

// #[tokio::test]
// async fn process_session() {
//     let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

//    // TODO(shahak): Change to GetBlocks::default() when the bug that forbids sending default
//    // messages is fixed.
//     let request = GetBlocks { limit: 10, ..Default::default() };
//     let request_id = OutboundSessionId::default();
//     let response = GetBlocksResponse {
//         response: Some(Response::Header(BlockHeader {
//             parent_block: Some(BlockId { hash: None, height: 1 }),
//             ..Default::default()
//         })),
//     };

//     let responses_sender =
//         start_request_and_validate_event(&mut handler, &request, request_id).await;

//     send_data_and_validate_event(&mut handler, &response, request_id, &responses_sender).await;
//     finish_session_and_validate_event(&mut handler, request_id).await;
// }

// #[tokio::test]
// async fn process_multiple_sessions_simultaneously() {
//     let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

//     const N_REQUESTS: usize = 20;
//     let request_ids = (0..N_REQUESTS).map(|value| OutboundSessionId { value
// }).collect::<Vec<_>>();     let requests = (0..N_REQUESTS)
//         .map(|i| GetBlocks { skip: i as u64, ..Default::default() })
//         .collect::<Vec<_>>();
//     let responses = (0..N_REQUESTS)
//         .map(|i| GetBlocksResponse {
//             response: Some(Response::Header(BlockHeader {
//                 parent_block: Some(BlockId { hash: None, height: i as u64 }),
//                 ..Default::default()
//             })),
//         })
//         .collect::<Vec<_>>();

//     for ((request, request_id), response) in zip(zip(requests, request_ids), responses.iter()) {
//         let responses_sender =
//             start_request_and_validate_event(&mut handler, &request, request_id).await;
//         responses_sender.unbounded_send(response.clone()).unwrap();
//     }

//     let mut request_id_found = [false; N_REQUESTS];
//     for event in handler.take(N_REQUESTS).collect::<Vec<_>>().await {
//         match event {
//             ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::ReceivedData {
//                 outbound_session_id: OutboundSessionId { value: i },
//                 data: event_data,
//             }) => {
//                 assert_eq!(responses[i], event_data);
//                 assert!(!request_id_found[i]);
//                 request_id_found[i] = true;
//             }
//             _ => {
//                 panic!("Got unexpected event");
//             }
//         }
//     }
// }
