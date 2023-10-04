use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec;

use assert_matches::assert_matches;
use futures::Stream;

use super::BehaviourTrait;
use crate::db_executor::{DBExecutor, Data, QueryId};
use crate::messages::protobuf;
use crate::streamed_data_protocol::{Config, OutboundSessionId, SessionId};
use crate::{block_headers_protocol, BlockHeader, BlockQuery, Signature};

// static mut test_db_executor: TestDBExecutor = TestDBExecutor {};

#[test]
fn header_message_to_header_or_signatures() {
    let test_db_executor = Arc::new(TestDBExecutor {});
    let behaviour =
        block_headers_protocol::Behaviour::new(Config::get_test_config(), test_db_executor);
    // TODO: add a get test instance method that returns a valid object
    let header_message =
        protobuf::block_headers_response_part::HeaderMessage::Header(protobuf::BlockHeader {
            parent_header: Some(protobuf::Hash { elements: [0x01].repeat(32) }),
            sequencer_address: Some(protobuf::Address { elements: [0x01].repeat(32) }),
            ..Default::default()
        });
    assert_matches!(
        behaviour.header_message_to_header_or_signatures(&header_message),
        Ok((Some(BlockHeader { .. }), None))
    );

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
fn positive_flow_batching_header_and_signatures() {
    let test_db_executor = Arc::new(TestDBExecutor {});
    let mut behaviour =
        block_headers_protocol::Behaviour::new(Config::get_test_config(), test_db_executor);
    let outbound_session_id_a = OutboundSessionId { value: 1 };
    let outbound_session_id_b = OutboundSessionId { value: 2 };
    let mut wait_to_complete_batching = false;

    let header_message =
        protobuf::block_headers_response_part::HeaderMessage::Header(protobuf::BlockHeader {
            parent_header: Some(protobuf::Hash { elements: [0x01].repeat(32) }),
            sequencer_address: Some(protobuf::Address { elements: [0x01].repeat(32) }),
            ..Default::default()
        });
    let signatures_message =
        protobuf::block_headers_response_part::HeaderMessage::Signatures(protobuf::Signatures {
            block: Some(protobuf::BlockId { number: 1, ..Default::default() }),
            signatures: vec![protobuf::ConsensusSignature {
                r: Some(protobuf::Felt252 { elements: [0x01].repeat(32) }),
                s: Some(protobuf::Felt252 { elements: [0x01].repeat(32) }),
            }],
        });

    // sending header to session a results in instruction to wait for batching
    assert_matches!(
        behaviour.handle_batching(
            outbound_session_id_a,
            &header_message,
            &mut wait_to_complete_batching
        ),
        block_headers_protocol::Event::SessionFailed { .. }
    );
    assert!(wait_to_complete_batching);

    // sending signatures to session b results in instruction to wait for batching
    assert_matches!(
        behaviour.handle_batching(
            outbound_session_id_b,
            &signatures_message,
            &mut wait_to_complete_batching
        ),
        block_headers_protocol::Event::SessionFailed { .. }
    );
    assert!(wait_to_complete_batching);

    // sending signatures to session a results in recieved data event and no instruction to wait for
    // batching
    assert_matches!(
        behaviour.handle_batching(
            outbound_session_id_a,
            &signatures_message,
            &mut wait_to_complete_batching,
        ),
        block_headers_protocol::Event::RecievedData { .. }
    );
    assert!(!wait_to_complete_batching);

    // sending header to session b results in recieved data event and no instruction to wait for
    // batching
    assert_matches!(
        behaviour.handle_batching(
            outbound_session_id_b,
            &header_message,
            &mut wait_to_complete_batching,
        ),
        block_headers_protocol::Event::RecievedData { .. }
    );
    assert!(!wait_to_complete_batching);
}

#[test]
#[ignore = "functionality not implemented completely yet"]
fn test_fin_handling() {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_pending_if_inner_behaviour_poll_is_pending() {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_pending_if_inner_behaviour_poll_is_ready_but_event_mapping_returns_wait_to_complete_batching()
 {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_ready_if_inner_behaviour_poll_is_ready_and_event_mapping_returns_not_to_wait_to_complete_batching()
 {
    unimplemented!()
}

struct TestBehaviour {
    should_wait: bool,
}

impl BehaviourTrait for TestBehaviour {
    fn map_inner_behaviour_event_to_own_event(
        &mut self,
        _in_event: crate::streamed_data_protocol::behaviour::Event<
            protobuf::BlockHeadersRequest,
            protobuf::BlockHeadersResponse,
        >,
        wait_to_complete_batching: &mut bool,
    ) -> block_headers_protocol::Event {
        *wait_to_complete_batching = self.should_wait;
        block_headers_protocol::Event::SessionCompletedSuccessfully {
            session_id: SessionId::OutboundSessionId(OutboundSessionId { value: 1 }),
        }
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
