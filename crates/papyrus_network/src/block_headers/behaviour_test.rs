use std::pin::Pin;
use std::task::{Context, Poll};

use assert_matches::assert_matches;
use futures::Stream;
use libp2p::PeerId;

use super::behaviour::BehaviourTrait;
use super::Event;
use crate::db_executor::{DBExecutor, Data, QueryId};
use crate::messages::{protobuf, ProtobufConversionError};
use crate::streamed_data::{self, OutboundSessionId, SessionId};
use crate::BlockQuery;

#[test]
#[ignore = "functionality not implemented completely yet"]
fn test_fin_handling() {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_pending_if_streamed_data_behaviour_poll_is_pending() {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_pending_if_streamed_data_behaviour_poll_is_ready_but_event_mapping_returns_wait_to_complete_pairing()
 {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_ready_if_streamed_data_behaviour_poll_is_ready_and_event_mapping_returns_not_to_wait_to_complete_pairing()
 {
    unimplemented!()
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_new_inbound_session() {
    let mut behaviour = TestBehaviour {};

    // send new inbound session event to behaviour from streamed data behaviour
    let peer_id = PeerId::random();
    let query = protobuf::BlockHeadersRequest {
        iteration: Some(protobuf::Iteration {
            start: Some(protobuf::iteration::Start::BlockNumber(1)),
            direction: 0,
            limit: 1,
            step: 1,
        }),
    };
    let inbound_session_id = streamed_data::InboundSessionId { value: rand::random() };
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::NewInboundSession {
        inbound_session_id,
        peer_id,
        query: query.clone(),
    };
    let mut ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );

    // Make sure we return the right event and call insert_inbound_session_id_to_waiting_list
    let converted_query: BlockQuery = query.try_into().unwrap();
    assert_matches!(
        res_event,
        Event::NewInboundQuery { query, inbound_session_id }
        if query == converted_query && inbound_session_id == inbound_session_id
    );

    // Send new inbound session event to behaviour from streamed data behaviour
    // but with bad query that can't be converted
    let peer_id = PeerId::random();
    let query = protobuf::BlockHeadersRequest::default();
    let inbound_session_id = streamed_data::InboundSessionId { value: rand::random() };
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::NewInboundSession {
        inbound_session_id,
        peer_id,
        query: query.clone(),
    };
    let mut ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::ProtobufConversionError(ProtobufConversionError::MissingField)
    );
}

struct TestBehaviour {}

#[allow(dead_code)]
impl BehaviourTrait for TestBehaviour {
    fn handle_received_data(
        &mut self,
        _data: protobuf::BlockHeadersResponse,
        _outbound_session_id: OutboundSessionId,
        _ignore_event_and_return_pending: &mut bool,
    ) -> Event {
        unimplemented!()
    }

    fn handle_session_closed_by_request(&mut self, _session_id: SessionId) -> Event {
        unimplemented!()
    }

    fn handle_outbound_session_closed_by_peer(
        &mut self,
        _outbound_session_id: OutboundSessionId,
    ) -> Event {
        unimplemented!()
    }
}

struct TestDBExecutor {}

impl Stream for TestDBExecutor {
    type Item = (QueryId, Data);

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

impl DBExecutor for TestDBExecutor {
    fn register_query(&mut self, _query: BlockQuery) -> QueryId {
        QueryId(1)
    }
}
