use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use assert_matches::assert_matches;
use futures::task::{Context, Poll};
use futures::{select, FutureExt, Stream as StreamTrait, StreamExt};
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedInbound};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, Stream};

use super::super::{DataBound, InboundSessionId, QueryBound};
use super::{Handler, HandlerEvent, RequestFromBehaviourEvent, ToBehaviourEvent};
use crate::messages::block::{GetBlocks, GetBlocksResponse};
use crate::messages::read_message;
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

fn simulate_new_inbound_session_from_swarm<Query: QueryBound, Data: DataBound>(
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

async fn validate_new_inbound_session_event<Query: QueryBound + PartialEq, Data: DataBound>(
    handler: &mut Handler<Query, Data>,
    query: &Query,
    inbound_session_id: InboundSessionId,
) {
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(ToBehaviourEvent::NewInboundSession {
            query: event_query, inbound_session_id: event_inbound_session_id
        }) if event_query == *query &&  event_inbound_session_id == inbound_session_id
    );
}

async fn read_messages(stream: &mut Stream, num_messages: usize) -> Vec<GetBlocksResponse> {
    let mut result = Vec::new();
    for _ in 0..num_messages {
        result.push(read_message::<GetBlocksResponse, _>(&mut *stream).await.unwrap());
    }
    result
}

#[tokio::test]
async fn process_inbound_session() {
    let mut handler = Handler::<GetBlocks, GetBlocksResponse>::new(
        SUBSTREAM_TIMEOUT,
        Arc::new(Default::default()),
    );

    let (inbound_stream, mut outbound_stream, _) = get_connected_streams().await;
    let query = GetBlocks::default();
    let inbound_session_id = InboundSessionId { value: 1 };

    simulate_new_inbound_session_from_swarm(
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

    let mut fused_handler = handler.fuse();
    let data_received = select! {
        data = read_messages(&mut outbound_stream, hardcoded_data_vec.len()).fuse() => data,
        _ = fused_handler.next() => panic!("There shouldn't be another event from the handler"),
    };
    assert_eq!(hardcoded_data_vec, data_received);
}

#[test]
fn listen_protocol_across_multiple_handlers() {
    let next_inbound_session_id = Arc::new(AtomicUsize::default());
    const NUM_HANDLERS: usize = 5;
    const NUM_PROTOCOLS_PER_HANDLER: usize = 10;
    let thread_handles = (0..NUM_HANDLERS).map(|_| {
        let next_inbound_session_id = next_inbound_session_id.clone();
        std::thread::spawn(|| {
            let handler = Handler::<GetBlocks, GetBlocksResponse>::new(
                SUBSTREAM_TIMEOUT,
                next_inbound_session_id,
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

//     let request = GetBlocks::default();
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
