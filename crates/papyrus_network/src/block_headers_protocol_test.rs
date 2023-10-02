use std::{time::Duration, vec};

use crate::{
    block_headers_protocol,
    messages::proto::p2p::proto::{self, block_headers_response},
    streamed_data_protocol::OutboundSessionId,
    BlockHeader, Signature,
};
use assert_matches::assert_matches;

use super::BehaviourTrait;

#[test]
fn header_message_to_header_or_signatures() {
    let behaviour = block_headers_protocol::Behaviour::new(Duration::MAX);
    //TODO: add a get test instance method that returns a valid object
    let header_message = block_headers_response::HeaderMessage::Header(proto::BlockHeader {
        parent_header: Some(proto::Hash { elements: vec![0x01].repeat(32) }),
        sequencer_address: Some(proto::Address { elements: vec![0x01].repeat(16) }),
        ..Default::default()
    });
    assert_matches!(
        behaviour.header_message_to_header_or_signatures(&header_message),
        Ok((Some(BlockHeader { .. }), None))
    );

    let broken_header_message = block_headers_response::HeaderMessage::Header(proto::BlockHeader {
        parent_header: Some(proto::Hash { elements: vec![0x01] }), // hash not long enough
        ..Default::default()
    });
    let res = behaviour.header_message_to_header_or_signatures(&broken_header_message);
    assert!(res.is_err());

    // TODO: add a get test instance method that returns valid object for HeaderMessage::Signatures, proto::Signatures and proto::ConcensusSignature
    let signatures_message = block_headers_response::HeaderMessage::Signatures(proto::Signatures {
        block_number: 1,
        signatures: vec![proto::ConsensusSignature {
            r: Some(proto::Felt252 { elements: vec![0x01].repeat(32) }),
            s: Some(proto::Felt252 { elements: vec![0x01].repeat(32) }),
        }],
    });
    assert_matches!(
        behaviour.header_message_to_header_or_signatures(&signatures_message),
        Ok((None, Some(Vec::<Signature> { .. })))
    );

    let broken_signatures_message =
        block_headers_response::HeaderMessage::Signatures(proto::Signatures {
            block_number: 1,
            signatures: vec![proto::ConsensusSignature {
                r: None,
                s: Some(proto::Felt252 { elements: vec![0x01].repeat(32) }),
            }],
        });
    let res = behaviour.header_message_to_header_or_signatures(&broken_signatures_message);
    assert!(res.is_err());
}

#[test]
fn positive_flow_batching_header_and_signatures() {
    let mut behaviour = block_headers_protocol::Behaviour::new(Duration::MAX);
    let outbound_session_id_a = OutboundSessionId { value: 1 };
    let outbound_session_id_b = OutboundSessionId { value: 2 };
    let mut wait_to_complete_batching = false;

    let header_message = block_headers_response::HeaderMessage::Header(proto::BlockHeader {
        parent_header: Some(proto::Hash { elements: vec![0x01].repeat(32) }),
        sequencer_address: Some(proto::Address { elements: vec![0x01].repeat(16) }),
        ..Default::default()
    });
    let signatures_message = block_headers_response::HeaderMessage::Signatures(proto::Signatures {
        block_number: 1,
        signatures: vec![proto::ConsensusSignature {
            r: Some(proto::Felt252 { elements: vec![0x01].repeat(32) }),
            s: Some(proto::Felt252 { elements: vec![0x01].repeat(32) }),
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

    // sending signatures to session a results in recieved data event and no instruction to wait for batching
    assert_matches!(
        behaviour.handle_batching(
            outbound_session_id_a,
            &signatures_message,
            &mut wait_to_complete_batching,
        ),
        block_headers_protocol::Event::RecievedData { .. }
    );
    assert!(!wait_to_complete_batching);

    // sending header to session b results in recieved data event and no instruction to wait for batching
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
fn test_fin_handling() {}

#[tokio::test]
async fn poll_is_pending_if_inner_behaviour_poll_is_pending() {}

#[tokio::test]
async fn poll_is_pending_if_inner_behaviour_poll_is_ready_but_event_mapping_returns_wait_to_complete_batching(
) {
}

#[tokio::test]
async fn poll_is_ready_if_inner_behaviour_poll_is_ready_and_event_mapping_returns_not_to_wait_to_complete_batching(
) {
}

struct TestBehaviour {
    should_wait: bool,
}

impl BehaviourTrait for TestBehaviour {
    fn map_generic_behaviour_event_to_specific_event(
        &mut self,
        _in_event: crate::streamed_data_protocol::behaviour::Event<
            crate::messages::block::BlockHeadersRequest,
            crate::messages::block::BlockHeadersResponse,
        >,
        wait_to_complete_batching: &mut bool,
    ) -> block_headers_protocol::Event {
        *wait_to_complete_batching = self.should_wait;
        block_headers_protocol::Event::SessionCompletedSuccessfully {
            outbound_session_id: OutboundSessionId { value: 1 },
        }
    }
}
