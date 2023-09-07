use std::iter::zip;
use std::pin::Pin;
use std::time::Duration;

use assert_matches::assert_matches;
use futures::channel::mpsc::UnboundedSender;
use futures::task::{Context, Poll};
use futures::{Stream, StreamExt};
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedOutbound};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent};

use super::super::RequestId;
use super::{Handler, HandlerEvent, NewRequestEvent, RequestProgressEvent};
use crate::db_executor::MockReaderExecutor;
use crate::messages::block::{BlockHeader, GetBlocks, GetBlocksResponse};
use crate::messages::common::BlockId;
use crate::messages::proto::p2p::proto::get_blocks_response::Response;

impl Unpin for Handler<MockReaderExecutor<Response>> {}

impl Stream for Handler<MockReaderExecutor<Response>> {
    type Item = HandlerEvent<Handler<MockReaderExecutor<Response>>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

async fn start_request_and_validate_event(
    handler: &mut Handler<MockReaderExecutor<Response>>,
    request: &GetBlocks,
    request_id: RequestId,
) -> UnboundedSender<GetBlocksResponse> {
    handler.on_behaviour_event(NewRequestEvent { request: request.clone(), request_id });
    let event = handler.next().await.unwrap();
    let ConnectionHandlerEvent::OutboundSubstreamRequest { protocol } = event else {
        panic!("Got unexpected event");
    };
    assert_eq!(*request, *protocol.upgrade().request());
    assert_eq!(SUBSTREAM_TIMEOUT, *protocol.timeout());
    protocol.upgrade().responses_sender().clone()
}

async fn send_response_and_validate_event(
    handler: &mut Handler<MockReaderExecutor<Response>>,
    response: &GetBlocksResponse,
    request_id: RequestId,
    responses_sender: &UnboundedSender<GetBlocksResponse>,
) {
    responses_sender.unbounded_send(response.clone()).unwrap();
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(RequestProgressEvent::ReceivedResponse{
            request_id: event_request_id, response: event_response
        }) if event_request_id == request_id && event_response == *response
    );
}

async fn finish_request_and_validate_event(
    handler: &mut Handler<MockReaderExecutor<Response>>,
    request_id: RequestId,
) {
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
        FullyNegotiatedOutbound { protocol: (), info: request_id },
    ));
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(RequestProgressEvent::RequestFinished{
            request_id: event_request_id
        }) if event_request_id == request_id
    );
}

#[tokio::test]
async fn process_request() {
    let mut handler = Handler::<MockReaderExecutor<Response>>::new(SUBSTREAM_TIMEOUT);

    let request = GetBlocks::default();
    let request_id = RequestId::default();
    let response = GetBlocksResponse {
        response: Some(Response::Header(BlockHeader {
            parent_block: Some(BlockId { hash: None, height: 1 }),
            ..Default::default()
        })),
    };

    let responses_sender =
        start_request_and_validate_event(&mut handler, &request, request_id).await;

    send_response_and_validate_event(&mut handler, &response, request_id, &responses_sender).await;
    finish_request_and_validate_event(&mut handler, request_id).await;
}

#[tokio::test]
async fn process_multiple_requests_simultaneously() {
    let mut handler = Handler::<MockReaderExecutor<Response>>::new(SUBSTREAM_TIMEOUT);

    const N_REQUESTS: usize = 20;
    let request_ids = (0..N_REQUESTS).map(RequestId).collect::<Vec<_>>();
    let requests = (0..N_REQUESTS)
        .map(|i| GetBlocks { skip: i as u64, ..Default::default() })
        .collect::<Vec<_>>();
    let responses = (0..N_REQUESTS)
        .map(|i| GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: i as u64 }),
                ..Default::default()
            })),
        })
        .collect::<Vec<_>>();

    for ((request, request_id), response) in zip(zip(requests, request_ids), responses.iter()) {
        let responses_sender =
            start_request_and_validate_event(&mut handler, &request, request_id).await;
        responses_sender.unbounded_send(response.clone()).unwrap();
    }

    let mut request_id_found = [false; N_REQUESTS];
    for event in handler.take(N_REQUESTS).collect::<Vec<_>>().await {
        match event {
            ConnectionHandlerEvent::NotifyBehaviour(RequestProgressEvent::ReceivedResponse {
                request_id: RequestId(i),
                response: event_response,
            }) => {
                assert_eq!(responses[i], event_response);
                assert!(!request_id_found[i]);
                request_id_found[i] = true;
            }
            _ => {
                panic!("Got unexpected event");
            }
        }
    }
}
