use std::collections::HashSet;

use assert_matches::assert_matches;
use libp2p::PeerId;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::hash::StarkFelt;

use super::super::Event;
use super::BehaviourTrait;
use crate::block_headers::SessionError;
use crate::messages::{protobuf, ProtobufConversionError, TestInstance};
use crate::streamed_data::{self, OutboundSessionId, SessionId};
use crate::{InternalQuery, SignedBlockHeader};

type StreamedDataEvent = streamed_data::GenericEvent<
    protobuf::BlockHeadersRequest,
    protobuf::BlockHeadersResponse,
    streamed_data::behaviour::SessionError,
>;

#[test]
fn map_streamed_data_behaviour_event_to_own_event_new_inbound_session() {
    let mut behaviour = TestBehaviour::new();

    // Send new inbound session event to behaviour from streamed data behaviour
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
    let streamed_data_event: StreamedDataEvent =
        streamed_data::behaviour::Event::NewInboundSession {
            inbound_session_id,
            peer_id,
            query: query.clone(),
        };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);

    // Make sure we return the right event and call insert_inbound_session_id_to_waiting_list
    let converted_query: InternalQuery = query.try_into().unwrap();
    assert_matches!(
        res_event,
        Some(Event::NewInboundQuery { query, inbound_session_id })
        if query == converted_query && inbound_session_id == inbound_session_id
    );

    // Send new inbound session event to behaviour from streamed data behaviour
    // but with bad query that can't be converted
    let peer_id = PeerId::random();
    let query = protobuf::BlockHeadersRequest::default();
    let inbound_session_id = streamed_data::InboundSessionId { value: rand::random() };
    let streamed_data_event: StreamedDataEvent =
        streamed_data::behaviour::Event::NewInboundSession {
            inbound_session_id,
            peer_id,
            query: query.clone(),
        };
    assert_eq!(behaviour.drop_session_call_count, 0);
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::QueryConversionError(ProtobufConversionError::MissingField))
    );
    assert_eq!(behaviour.drop_session_call_count, 1);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_simple_happy_flow() {
    let mut behaviour = TestBehaviour::new();
    let outbound_session_id = OutboundSessionId { value: rand::random() };

    // Send header response event to behaviour from streamed data behaviour
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            header_message: Some(protobuf::block_headers_response::HeaderMessage::Header(
                protobuf::SignedBlockHeader::test_instance(),
            )),
        },
    };

    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::ReceivedData { signed_header, outbound_session_id: session_id}) => {
            assert_matches!(signed_header, SignedBlockHeader { block_header, signatures}
                if block_header.block_number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].0.r == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap() &&
                signatures[0].0.s == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap());
            assert_eq!(outbound_session_id, session_id);
        }
    );

    // Send fin event to behaviour from streamed data behaviour
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            header_message: Some(protobuf::block_headers_response::HeaderMessage::Fin(
                protobuf::Fin {},
            )),
        },
    };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(res_event, None);

    // Make sure no function was called unexpectedly
    assert_eq!(behaviour.drop_session_call_count, 0);
    assert_eq!(behaviour.handle_session_finished_call_count, 0);

    // TODO(shahak): Investigate why this causes a failure and uncomment.
    // let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
    //     streamed_data::behaviour::Event::SessionFinishedSuccessfully {
    //         session_id: outbound_session_id.into(),
    //     },
    // );

    // assert_matches!(
    //     res_event,
    //     Some(Event::SessionFinishedSuccessfully { session_id }) => {
    //         assert_eq!(SessionId::from(outbound_session_id), session_id);
    //     }
    // );
    // assert_eq!(behaviour.drop_session_call_count, 0);
    // assert_eq!(behaviour.handle_session_finished_call_count, 1);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_protobuf_conversion_error() {
    let mut behaviour = TestBehaviour::new();

    // Send bad header message - should return incompatible data error event
    let outbound_session_id = OutboundSessionId { value: rand::random() };
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse { header_message: None },
    };

    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::SessionFailed {
            session_id,
            session_error,
        }) => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(
                session_error,
                SessionError::ProtobufConversionError(ProtobufConversionError::MissingField)
            )
        }
    );
}

struct TestBehaviour {
    handle_session_finished_call_count: usize,
    drop_session_call_count: usize,
    sessions_pending_termination: HashSet<SessionId>,
}

impl TestBehaviour {
    fn new() -> Self {
        Self {
            handle_session_finished_call_count: 0,
            drop_session_call_count: 0,
            sessions_pending_termination: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.handle_session_finished_call_count = 0;
        self.drop_session_call_count = 0;
    }
}

impl BehaviourTrait for TestBehaviour {
    fn handle_session_finished(&mut self, _session_id: SessionId) -> Option<Event> {
        self.handle_session_finished_call_count += 1;
        None
    }

    fn drop_session(&mut self, _session_id: SessionId) {
        self.drop_session_call_count += 1;
    }

    fn get_sessions_pending_termination(&mut self) -> &mut HashSet<SessionId> {
        &mut self.sessions_pending_termination
    }
}
