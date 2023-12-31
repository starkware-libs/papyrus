use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec;

use assert_matches::assert_matches;
use futures::Stream;

use super::behaviour::BehaviourTrait;
use crate::block_headers::{BlockHeader, Signature};
use crate::db_executor::{DBExecutor, Data, QueryId};
use crate::messages::protobuf;
use crate::streamed_data::{Config, OutboundSessionId, SessionId};
use crate::{block_headers, BlockQuery};

// static mut test_db_executor: TestDBExecutor = TestDBExecutor {};

#[test]
fn header_message_to_header_or_signatures() {
    let test_db_executor = Arc::new(TestDBExecutor {});
    let behaviour =
        block_headers::behaviour::Behaviour::new(Config::get_test_config(), test_db_executor);
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
async fn poll_is_pending_if_streamed_data_behaviour_poll_is_ready_but_event_mapping_returns_wait_to_complete_pairing(
) {
    unimplemented!()
}

#[tokio::test]
#[ignore = "functionality not implemented completely yet"]
async fn poll_is_ready_if_streamed_data_behaviour_poll_is_ready_and_event_mapping_returns_not_to_wait_to_complete_pairing(
) {
    unimplemented!()
}

struct TestBehaviour {
    should_wait: bool,
}

impl BehaviourTrait for TestBehaviour {
    fn map_streamed_data_behaviour_event_to_own_event(
        &mut self,
        _in_event: crate::streamed_data::behaviour::Event<
            protobuf::BlockHeadersRequest,
            protobuf::BlockHeadersResponse,
        >,
        wait_to_complete_pairing: &mut bool,
    ) -> block_headers::Event {
        *wait_to_complete_pairing = self.should_wait;
        block_headers::Event::SessionCompletedSuccessfully {
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
