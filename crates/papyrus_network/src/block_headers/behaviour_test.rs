use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;

use super::behaviour::BehaviourTrait;
use super::Event;
use crate::db_executor::{DBExecutor, Data, QueryId};
use crate::messages::protobuf;
use crate::streamed_data::{InboundSessionId, OutboundSessionId, SessionId};
use crate::BlockQuery;

// static mut test_db_executor: TestDBExecutor = TestDBExecutor {};

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

struct TestBehaviour {}

impl BehaviourTrait<TestDBExecutor> for TestBehaviour {
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

    fn handle_new_inbound_session(
        &mut self,
        _query: protobuf::BlockHeadersRequest,
        _inbound_session_id: InboundSessionId,
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
