use std::collections::{HashMap, HashSet};

use assert_matches::assert_matches;
use libp2p::PeerId;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_types_core::felt::Felt;

use super::super::Event;
use super::BehaviourTrait;
use crate::block_headers::{BlockHeaderData, SessionError};
use crate::messages::{protobuf, ProtobufConversionError, TestInstance};
use crate::streamed_data::{self, OutboundSessionId, SessionId};
use crate::BlockQuery;

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
    let converted_query: BlockQuery = query.try_into().unwrap();
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
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };

    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(res_event, None);
    assert_eq!(behaviour.store_header_pending_pairing_with_signature_call_count, 1);

    // Send matching signature response event to behaviour from streamed data behaviour
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(
                    protobuf::block_headers_response_part::HeaderMessage::Signatures(
                        protobuf::Signatures::test_instance(),
                    ),
                ),
            }],
        },
    };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::ReceivedData {data, outbound_session_id: session_id}) => {
            assert_matches!(data.first().unwrap(), BlockHeaderData { block_header, signatures}
                if block_header.block_number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].r == Felt::from_bytes_be(&[1].repeat(32).to_vec().try_into().unwrap()) &&
                signatures[0].s == Felt::from_bytes_be(&[1].repeat(32).to_vec().try_into().unwrap()));
            assert_eq!(outbound_session_id, session_id);
        }
    );
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 1);

    // Send fin event to behaviour from streamed data behaviour
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Fin(
                    protobuf::Fin { error: None },
                )),
            }],
        },
    };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(res_event, None);

    // Make sure no function was called unexpectedly
    assert_eq!(behaviour.store_header_pending_pairing_with_signature_call_count, 1);
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 1);
    assert_eq!(behaviour.drop_session_call_count, 0);
    assert_eq!(behaviour.handle_session_finished_call_count, 0);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_happy_flow_two_sessions() {
    let mut behaviour = TestBehaviour::new();
    let outbound_session_id_a = OutboundSessionId { value: rand::random() };
    let outbound_session_id_b = OutboundSessionId { value: rand::random() };

    // Send header response event to behaviour from streamed data behaviour - session A
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id: outbound_session_id_a,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };

    let _res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);

    // Send header response event to behaviour from streamed data behaviour - session B
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id: outbound_session_id_b,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };

    let _res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);

    // Send matching signature response event to behaviour from streamed data behaviour - Session B
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id: outbound_session_id_b,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(
                    protobuf::block_headers_response_part::HeaderMessage::Signatures(
                        protobuf::Signatures::test_instance(),
                    ),
                ),
            }],
        },
    };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::ReceivedData {data, outbound_session_id: session_id}) => {
            assert_matches!(data.first().unwrap(), BlockHeaderData { block_header, signatures}
                if block_header.block_number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].r == Felt::from_bytes_be(&[1].repeat(32).to_vec().try_into().unwrap()) &&
                signatures[0].s == Felt::from_bytes_be(&[1].repeat(32).to_vec().try_into().unwrap()));
            assert_eq!(outbound_session_id_b, session_id);
        }
    );
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 1);

    // Send matching signature response event to behaviour from streamed data behaviour - Session A
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id: outbound_session_id_a,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(
                    protobuf::block_headers_response_part::HeaderMessage::Signatures(
                        protobuf::Signatures::test_instance(),
                    ),
                ),
            }],
        },
    };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::ReceivedData {data, outbound_session_id: session_id}) => {
            assert_matches!(data.first().unwrap(), BlockHeaderData { block_header, signatures}
                if block_header.block_number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].r == Felt::from_bytes_be(&[1].repeat(32).to_vec().try_into().unwrap()) &&
                signatures[0].s == Felt::from_bytes_be(&[1].repeat(32).to_vec().try_into().unwrap()));
            assert_eq!(outbound_session_id_a, session_id);
        }
    );
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 2);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_pairing_error() {
    let mut behaviour = TestBehaviour::new();
    let outbound_session_id = OutboundSessionId { value: rand::random() };

    // Send signature response event to behaviour from streamed data
    // behaviour before header response event - should return pairing error event
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(
                    protobuf::block_headers_response_part::HeaderMessage::Signatures(
                        protobuf::Signatures::test_instance(),
                    ),
                ),
            }],
        },
    };

    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::SessionFailed {
            session_id,
            session_error,
        }) => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(session_error, SessionError::PairingError)
        }
    );
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_incompatible_data() {
    let mut behaviour = TestBehaviour::new();

    // Send bad header message - should return incompatible data error event
    let outbound_session_id = OutboundSessionId { value: rand::random() };
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart { header_message: None }],
        },
    };

    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::SessionFailed {
            session_id,
            session_error,
        }) => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(session_error, SessionError::IncompatibleDataError)
        }
    );

    // Send header to match signature to
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };
    let _res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);

    // Send bad signature message - should return ProtobufConversionError
    let streamed_data_event: StreamedDataEvent = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(
                    protobuf::block_headers_response_part::HeaderMessage::Signatures(
                        protobuf::Signatures {
                            block: Some(protobuf::BlockId { number: 1, header: None }),
                            signatures: vec![protobuf::ConsensusSignature { r: None, s: None }],
                        },
                    ),
                ),
            }],
        },
    };
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(streamed_data_event);
    assert_matches!(
        res_event,
        Some(Event::SessionFailed {
            session_id,
            session_error,
        }) => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(session_error, SessionError::ProtobufConversionError(ProtobufConversionError::MissingField))
        }
    );
}

struct TestBehaviour {
    store_header_pending_pairing_with_signature_call_count: usize,
    fetch_pending_header_for_session_call_count: usize,
    handle_session_finished_call_count: usize,
    drop_session_call_count: usize,
    header_pending_pairing: HashMap<OutboundSessionId, protobuf::BlockHeader>,
    sessions_pending_termination: HashSet<SessionId>,
}

impl TestBehaviour {
    fn new() -> Self {
        Self {
            store_header_pending_pairing_with_signature_call_count: 0,
            fetch_pending_header_for_session_call_count: 0,
            handle_session_finished_call_count: 0,
            drop_session_call_count: 0,
            header_pending_pairing: HashMap::new(),
            sessions_pending_termination: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.store_header_pending_pairing_with_signature_call_count = 0;
        self.fetch_pending_header_for_session_call_count = 0;
        self.handle_session_finished_call_count = 0;
        self.drop_session_call_count = 0;
        self.header_pending_pairing = HashMap::new();
    }
}

impl BehaviourTrait for TestBehaviour {
    fn fetch_header_pending_pairing_with_signature(
        &mut self,
        outbound_session_id: OutboundSessionId,
    ) -> Result<BlockHeader, SessionError> {
        self.fetch_pending_header_for_session_call_count += 1;
        self.header_pending_pairing
            .remove(&outbound_session_id)
            .and_then(|header| TryInto::<BlockHeader>::try_into(header).ok())
            .ok_or(SessionError::PairingError)
    }

    fn store_header_pending_pairing_with_signature(
        &mut self,
        header: protobuf::BlockHeader,
        outbound_session_id: OutboundSessionId,
    ) -> Result<(), SessionError> {
        self.store_header_pending_pairing_with_signature_call_count += 1;
        self.header_pending_pairing
            .insert(outbound_session_id, header.clone())
            .map(|_| ())
            .xor(Some(()))
            .ok_or_else(|| SessionError::PairingError)
    }

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
