use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;

use super::behaviour::BehaviourTrait;
use super::Event;
use crate::db_executor::{DBExecutor, Data, QueryId};
use crate::messages::protobuf;
use crate::streamed_data::{InboundSessionId, OutboundSessionId, SessionId};
use crate::BlockQuery;

    let broken_header_message =
        protobuf::block_headers_response_part::HeaderMessage::Header(protobuf::BlockHeader {
            parent_header: Some(protobuf::Hash { elements: vec![0x01] }), // hash not long enough
            ..Default::default()
        });
    let res = behaviour.header_message_to_header_or_signatures(&broken_header_message);
    assert!(res.is_err());

    // TODO: add a get test instance method that returns valid object for HeaderMessage::Signatures,
    // protobuf::Signatures and protobuf::ConcensusSignature
    let signatures_message =
        protobuf::block_headers_response_part::HeaderMessage::Signatures(protobuf::Signatures {
            block: Some(protobuf::BlockId { number: 1, ..Default::default() }),
            signatures: vec![protobuf::ConsensusSignature {
                r: Some(protobuf::Felt252 { elements: [0x01].repeat(32) }),
                s: Some(protobuf::Felt252 { elements: [0x01].repeat(32) }),
            }],
        });
    assert_matches!(
        behaviour.header_message_to_header_or_signatures(&signatures_message),
        Ok((None, Some(Vec::<Signature> { .. })))
    );

    let broken_signatures_message =
        protobuf::block_headers_response_part::HeaderMessage::Signatures(protobuf::Signatures {
            block: Some(protobuf::BlockId { number: 1, ..Default::default() }),
            signatures: vec![protobuf::ConsensusSignature {
                r: None,
                s: Some(protobuf::Felt252 { elements: [0x01].repeat(32) }),
            }],
        });
    let res = behaviour.header_message_to_header_or_signatures(&broken_signatures_message);
    assert!(res.is_err());
}

#[test]
>>>>>>> 1900c987 (chore(network): rename block_header behaviour test file)
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
