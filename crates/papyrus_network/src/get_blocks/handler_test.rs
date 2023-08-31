use std::iter::zip;
use std::pin::Pin;
use std::time::Duration;

use assert_matches::assert_matches;
use futures::channel::mpsc::{unbounded, TrySendError, UnboundedSender};
use futures::task::{Context, Poll};
use futures::{FutureExt, Stream, StreamExt};
use libp2p::swarm::handler::{ConnectionEvent, DialUpgradeError, FullyNegotiatedOutbound};
use libp2p::swarm::{
    ConnectionHandler,
    ConnectionHandlerEvent,
    StreamUpgradeError,
    SubstreamProtocol,
};

use super::super::protocol::{RequestProtocol, RequestProtocolError};
use super::super::RequestId;
use super::{
    Handler,
    HandlerEvent,
    NewRequestEvent,
    RemoteDoesntSupportProtocolError,
    RequestError,
    RequestProgressEvent,
};
use crate::messages::block::{BlockHeader, GetBlocks, GetBlocksResponse};
use crate::messages::common::BlockId;
use crate::messages::proto::p2p::proto::get_blocks_response::Response;

impl Unpin for Handler {}

impl Stream for Handler {
    type Item = HandlerEvent<Handler>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(event) => Poll::Ready(Some(event)),
        }
    }
}

// Implementing PartialEq manually for RequestProtocol because we want to ignore `responses_sender`.
impl PartialEq for RequestProtocol {
    fn eq(&self, other: &Self) -> bool {
        return self.request() == other.request();
    }
}

// Implementing PartialEq manually for RequestError because std::io::Error doesn't impl Eq. Instead,
// we'll only compare the error kind for std::io::Error.
impl PartialEq for RequestError {
    fn eq(&self, other: &Self) -> bool {
        match self {
            RequestError::Timeout { substream_timeout } => {
                if let RequestError::Timeout { substream_timeout: other_substream_timeout } = other
                {
                    return substream_timeout == other_substream_timeout;
                }
                false
            }
            RequestError::IOError(error) => {
                if let RequestError::IOError(other_error) = other {
                    // Since std::io::Error doesn't impl Eq, we'll only compare kind.
                    return error.kind() == other_error.kind();
                }
                false
            }
            RequestError::ResponseSendError(error) => {
                if let RequestError::ResponseSendError(other_error) = other {
                    return error == other_error;
                }
                false
            }
            RequestError::RemoteDoesntSupportProtocol => {
                matches!(other, RequestError::RemoteDoesntSupportProtocol)
            }
        }
    }
}

const SUBSTREAM_TIMEOUT: Duration = Duration::MAX;

async fn start_request_and_validate_event(
    handler: &mut Handler,
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
    handler: &mut Handler,
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

async fn finish_request_and_validate_event(handler: &mut Handler, request_id: RequestId) {
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
    let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

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
async fn failure_in_request() {
    async fn get_try_send_error() -> TrySendError<GetBlocksResponse> {
        let (responses_sender, mut responses_receiver) = unbounded();
        responses_receiver.close();
        let response = GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 1 }),
                ..Default::default()
            })),
        };
        responses_sender.unbounded_send(response).unwrap_err()
    }

    async fn validate_handler_events_one_of_possible_events(
        handler: &mut Handler,
        possible_expected_events: &Vec<Vec<&HandlerEvent<Handler>>>,
        len_expected_events: usize,
    ) {
        let mut handler_events = Vec::<HandlerEvent<Handler>>::new();
        for _ in 0..len_expected_events {
            handler_events.push(handler.next().await.unwrap());
        }
        let mut should_panic = true;
        for events in possible_expected_events {
            let events_fit = zip(events.iter(), handler_events.iter())
                .all(|(event, handler_event)| *(*event) == *handler_event);
            if events_fit {
                should_panic = false;
                break;
            }
        }
        assert!(!should_panic);
        assert!(handler.next().now_or_never().is_none());
    }

    let io_error_kind = std::io::ErrorKind::InvalidData;
    let try_send_error = get_try_send_error().await;

    for (p2p_error, expected_error, should_handler_close) in [
        (
            StreamUpgradeError::Timeout,
            RequestError::Timeout { substream_timeout: SUBSTREAM_TIMEOUT },
            false,
        ),
        (
            StreamUpgradeError::Io(io_error_kind.into()),
            RequestError::IOError(io_error_kind.into()),
            false,
        ),
        (
            StreamUpgradeError::Apply(RequestProtocolError::IOError(io_error_kind.into())),
            RequestError::IOError(io_error_kind.into()),
            false,
        ),
        (
            StreamUpgradeError::Apply(RequestProtocolError::ResponseSendError(
                try_send_error.clone(),
            )),
            RequestError::ResponseSendError(try_send_error),
            false,
        ),
        (StreamUpgradeError::NegotiationFailed, RequestError::RemoteDoesntSupportProtocol, true),
    ] {
        let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

        let request = GetBlocks::default();
        let request_id = RequestId(0);
        let response = GetBlocksResponse {
            response: Some(Response::Header(BlockHeader {
                parent_block: Some(BlockId { hash: None, height: 1 }),
                ..Default::default()
            })),
        };

        let responses_sender =
            start_request_and_validate_event(&mut handler, &request, request_id).await;

        // We want to check that if a request fails that there won't be any events from that request
        // but there will be events from other requests. In order to do that, we'll add events from
        // this request and from other requests and then report the failure.
        let other_request_id1 = RequestId(1);
        let other_request_id2 = RequestId(2);
        let other_responses_sender =
            start_request_and_validate_event(&mut handler, &request, other_request_id1).await;
        other_responses_sender.unbounded_send(response.clone()).unwrap();
        responses_sender.unbounded_send(response.clone()).unwrap();
        handler.on_behaviour_event(NewRequestEvent {
            request: request.clone(),
            request_id: other_request_id2,
        });

        handler.on_connection_event(ConnectionEvent::DialUpgradeError(DialUpgradeError {
            info: request_id,
            error: p2p_error,
        }));

        let expected_failure_event =
            ConnectionHandlerEvent::NotifyBehaviour(RequestProgressEvent::RequestFailed {
                request_id,
                error: expected_error,
            });
        let expected_other_event1 =
            ConnectionHandlerEvent::NotifyBehaviour(RequestProgressEvent::ReceivedResponse {
                request_id: other_request_id1,
                response,
            });
        let expected_other_event2 = ConnectionHandlerEvent::OutboundSubstreamRequest {
            protocol: SubstreamProtocol::new(RequestProtocol::new(request).0, other_request_id2)
                .with_timeout(SUBSTREAM_TIMEOUT),
        };
        let (possible_expected_events, len_expected_events) = if should_handler_close {
            (
                vec![vec![
                    &expected_failure_event,
                    &ConnectionHandlerEvent::Close(RemoteDoesntSupportProtocolError),
                ]],
                2,
            )
        } else {
            (
                vec![
                    vec![&expected_other_event1, &expected_other_event2, &expected_failure_event],
                    vec![&expected_other_event2, &expected_other_event1, &expected_failure_event],
                    vec![&expected_other_event1, &expected_failure_event, &expected_other_event2],
                    vec![&expected_other_event2, &expected_failure_event, &expected_other_event1],
                    vec![&expected_failure_event, &expected_other_event1, &expected_other_event2],
                    vec![&expected_failure_event, &expected_other_event2, &expected_other_event1],
                ],
                3,
            )
        };

        validate_handler_events_one_of_possible_events(
            &mut handler,
            &possible_expected_events,
            len_expected_events,
        )
        .await;
    }
}

#[tokio::test]
async fn process_multiple_requests_simultaneously() {
    let mut handler = Handler::new(SUBSTREAM_TIMEOUT);

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
