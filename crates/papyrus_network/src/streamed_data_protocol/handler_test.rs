use std::iter::zip;
use std::pin::Pin;
use std::time::Duration;

use assert_matches::assert_matches;
use futures::channel::mpsc::UnboundedSender;
use futures::task::{Context, Poll};
use futures::{Stream, StreamExt};
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedOutbound};
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent};
use prost::Message;

use super::super::OutboundSessionId;
use super::{Handler, HandlerEvent, NewQueryEvent, SessionProgressEvent};
use crate::messages::block::{BlockHeader, GetBlocks, GetBlocksResponse};
use crate::messages::common::BlockId;
use crate::messages::proto::p2p::proto::get_blocks_response::Response;

impl<Query: Message, Data: Message + Default> Unpin for Handler<Query, Data> {}

impl<Query: Message, Data: Message + Default> Stream for Handler<Query, Data> {
    type Item = HandlerEvent<Handler<Query, Data>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

async fn start_request_and_validate_event<
    Query: Message + PartialEq + Clone,
    Data: Message + Default,
>(
    handler: &mut Handler<Query, Data>,
    query: &Query,
    outbound_session_id: OutboundSessionId,
) -> UnboundedSender<Data> {
    handler.on_behaviour_event(NewQueryEvent { query: query.clone(), outbound_session_id });
    let event = handler.next().await.unwrap();
    let ConnectionHandlerEvent::OutboundSubstreamRequest { protocol } = event else {
        panic!("Got unexpected event");
    };
    assert_eq!(*query, *protocol.upgrade().query());
    assert_eq!(SUBSTREAM_TIMEOUT, *protocol.timeout());
    protocol.upgrade().data_sender().clone()
}

async fn send_data_and_validate_event<
    Query: Message,
    Data: Message + Default + PartialEq + Clone,
>(
    handler: &mut Handler<Query, Data>,
    data: &Data,
    outbound_session_id: OutboundSessionId,
    data_sender: &UnboundedSender<Data>,
) {
    data_sender.unbounded_send(data.clone()).unwrap();
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::ReceivedData{
            outbound_session_id: event_outbound_session_id, data: event_data
        }) if event_outbound_session_id == outbound_session_id && event_data == *data
    );
}

async fn finish_session_and_validate_event<Query: Message, Data: Message + Default>(
    handler: &mut Handler<Query, Data>,
    outbound_session_id: OutboundSessionId,
) {
    handler.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
        FullyNegotiatedOutbound { protocol: (), info: outbound_session_id },
    ));
    let event = handler.next().await.unwrap();
    assert_matches!(
        event,
        ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::SessionFinished{
            outbound_session_id: event_outbound_session_id
        }) if event_outbound_session_id == outbound_session_id
    );
}

#[tokio::test]
async fn process_session() {
    let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

    let request = GetBlocks::default();
    let request_id = OutboundSessionId::default();
    let response = GetBlocksResponse {
        response: Some(Response::Header(BlockHeader {
            parent_block: Some(BlockId { hash: None, height: 1 }),
            ..Default::default()
        })),
    };

    let responses_sender =
        start_request_and_validate_event(&mut handler, &request, request_id).await;

    send_data_and_validate_event(&mut handler, &response, request_id, &responses_sender).await;
    finish_session_and_validate_event(&mut handler, request_id).await;
}

#[tokio::test]
async fn process_multiple_sessions_simultaneously() {
    let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

    const N_REQUESTS: usize = 20;
    let request_ids = (0..N_REQUESTS).map(|value| OutboundSessionId { value }).collect::<Vec<_>>();
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
            ConnectionHandlerEvent::NotifyBehaviour(SessionProgressEvent::ReceivedData {
                outbound_session_id: OutboundSessionId { value: i },
                data: event_data,
            }) => {
                assert_eq!(responses[i], event_data);
                assert!(!request_id_found[i]);
                request_id_found[i] = true;
            }
            _ => {
                panic!("Got unexpected event");
            }
        }
    }
}
