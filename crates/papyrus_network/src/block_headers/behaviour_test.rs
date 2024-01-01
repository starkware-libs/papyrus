use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use assert_matches::assert_matches;
use futures::Stream;
use libp2p::PeerId;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::hash::StarkFelt;

use super::behaviour::BehaviourTrait;
use super::Event;
use crate::block_headers::{BlockHeaderData, SessionError};
use crate::db_executor::{DBExecutor, Data, QueryId};
use crate::messages::{protobuf, ProtobufConversionError, TestInstance};
use crate::streamed_data::{self, OutboundSessionId, SessionId};
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

#[test]
fn map_streamed_data_behaviour_event_to_own_event_new_inbound_session() {
    let mut behaviour = TestBehaviour::new();

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

    // make sure we return the right event and call insert_inbound_session_id_to_waiting_list
    let converted_query: BlockQuery = query.try_into().unwrap();
    assert_matches!(
        res_event,
        Event::NewInboundQuery { query, inbound_session_id }
        if query == converted_query && inbound_session_id == inbound_session_id
    );
    assert_eq!(behaviour.insert_inbound_session_id_to_waiting_list_call_count, 1);
    behaviour.reset();

    // send new inbound session event to behaviour from streamed data behaviour
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
    assert_eq!(behaviour.insert_inbound_session_id_to_waiting_list_call_count, 0);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_simple_happy_flow() {
    let mut behaviour = TestBehaviour::new();
    let outbound_session_id = OutboundSessionId { value: rand::random() };

    // send header response event to behaviour from streamed data behaviour
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };
    let mut ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::SessionFailed {session_id, session_error} => {
            assert_matches!(session_error, SessionError::WaitingToCompletePairing);
            assert_eq!(SessionId::OutboundSessionId(outbound_session_id), session_id);
        }
    );
    assert_eq!(behaviour.store_header_pending_pairing_with_signature_call_count, 1);
    assert!(ignore_event_and_return_pending);

    // send matching signature response event to behaviour from streamed data behaviour
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
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
    ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::RecievedData {data, outbound_session_id: session_id} => {
            assert_matches!(data, BlockHeaderData { block_header, signatures}
                if block_header.number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].r == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap() &&
                signatures[0].s == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap());
            assert_eq!(outbound_session_id, session_id);
        }
    );
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 1);
    assert!(!ignore_event_and_return_pending);

    // send fin event to behaviour from streamed data behaviour
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Fin(
                    protobuf::Fin { error: None },
                )),
            }],
        },
    };
    ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::SessionFailed {session_id, session_error} => {
            assert_matches!(session_error, SessionError::ReceivedFin);
            assert_eq!(SessionId::OutboundSessionId(outbound_session_id), session_id);
        }
    );
    assert_eq!(behaviour.close_outbound_session_call_count, 1);

    // make sure no function was called unexpectedly
    assert_eq!(behaviour.store_header_pending_pairing_with_signature_call_count, 1);
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 1);
    assert_eq!(behaviour.close_outbound_session_call_count, 1);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_happy_flow_two_sessions() {
    let mut behaviour = TestBehaviour::new();
    let outbound_session_id_a = OutboundSessionId { value: rand::random() };
    let outbound_session_id_b = OutboundSessionId { value: rand::random() };

    // send header response event to behaviour from streamed data behaviour - session A
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id: outbound_session_id_a,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };
    let mut ignore_event_and_return_pending = false;
    let _res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );

    // send header response event to behaviour from streamed data behaviour - session B
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id: outbound_session_id_b,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };
    let mut ignore_event_and_return_pending = false;
    let _res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );

    // send matching signature response event to behaviour from streamed data behaviour - Session B
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
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
    ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::RecievedData {data, outbound_session_id: session_id} => {
            assert_matches!(data, BlockHeaderData { block_header, signatures}
                if block_header.number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].r == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap() &&
                signatures[0].s == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap());
            assert_eq!(outbound_session_id_b, session_id);
        }
    );
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 1);
    assert!(!ignore_event_and_return_pending);

    // send matching signature response event to behaviour from streamed data behaviour - Session A
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
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
    ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::RecievedData {data, outbound_session_id: session_id} => {
            assert_matches!(data, BlockHeaderData { block_header, signatures}
                if block_header.number == BlockNumber(1) && signatures.len() == 1 &&
                signatures[0].r == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap() &&
                signatures[0].s == StarkFelt::new([1].repeat(32).to_vec().try_into().unwrap()).unwrap());
            assert_eq!(outbound_session_id_a, session_id);
        }
    );
    assert_eq!(behaviour.fetch_pending_header_for_session_call_count, 2);
    assert!(!ignore_event_and_return_pending);
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_bad_flow() {
    let mut behaviour = TestBehaviour::new();
    let outbound_session_id = OutboundSessionId { value: rand::random() };

    // send signature response event to behaviour from streamed data behaviour
    // behaviour before header response event - should return pairing error event
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
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
    let mut ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::SessionFailed {
            session_id,
            session_error,
        } => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(session_error, SessionError::PairingError)
        }
    );
}

#[test]
fn map_streamed_data_behaviour_event_to_own_event_recieve_data_bad_objects() {
    let mut behaviour = TestBehaviour::new();

    // send bad header message - should return conversion error event
    let outbound_session_id = OutboundSessionId { value: rand::random() };
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart { header_message: None }],
        },
    };
    let mut ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::SessionFailed {
            session_id,
            session_error,
        } => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(session_error, SessionError::StreamedDataEventConversionError)
        }
    );

    // send header to match signature to
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
        outbound_session_id,
        data: protobuf::BlockHeadersResponse {
            part: vec![protobuf::BlockHeadersResponsePart {
                header_message: Some(protobuf::block_headers_response_part::HeaderMessage::Header(
                    protobuf::BlockHeader::test_instance(),
                )),
            }],
        },
    };
    ignore_event_and_return_pending = false;
    let _res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );

    // send bad signature message - should return pairing error event
    let streamed_data_event: streamed_data::GenericEvent<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
        streamed_data::behaviour::SessionError,
    > = streamed_data::behaviour::Event::ReceivedData {
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
    ignore_event_and_return_pending = false;
    let res_event = behaviour.map_streamed_data_behaviour_event_to_own_event(
        streamed_data_event,
        &mut ignore_event_and_return_pending,
    );
    assert_matches!(
        res_event,
        Event::SessionFailed {
            session_id,
            session_error,
        } => {
            assert_eq!(session_id, outbound_session_id.into());
            assert_matches!(session_error, SessionError::StreamedDataEventConversionError)
        }
    );
}

struct TestBehaviour {
    insert_inbound_session_id_to_waiting_list_call_count: usize,
    store_header_pending_pairing_with_signature_call_count: usize,
    fetch_pending_header_for_session_call_count: usize,
    close_outbound_session_call_count: usize,
    header_pending_pairing: HashMap<OutboundSessionId, protobuf::BlockHeader>,
}

impl TestBehaviour {
    fn new() -> Self {
        Self {
            insert_inbound_session_id_to_waiting_list_call_count: 0,
            store_header_pending_pairing_with_signature_call_count: 0,
            fetch_pending_header_for_session_call_count: 0,
            close_outbound_session_call_count: 0,
            header_pending_pairing: HashMap::new(),
        }
    }

    fn reset(&mut self) {
        self.insert_inbound_session_id_to_waiting_list_call_count = 0;
        self.store_header_pending_pairing_with_signature_call_count = 0;
        self.fetch_pending_header_for_session_call_count = 0;
        self.close_outbound_session_call_count = 0;
        self.header_pending_pairing = HashMap::new();
    }
}

#[allow(dead_code)]
impl BehaviourTrait<TestDBExecutor> for TestBehaviour {
    fn handle_session_closed_by_request(&mut self, _session_id: SessionId) -> Event {
        unimplemented!()
    }

    fn handle_outbound_session_closed_by_peer(
        &mut self,
        _outbound_session_id: OutboundSessionId,
    ) -> Event {
        unimplemented!()
    }

    fn insert_inbound_session_id_to_waiting_list(
        &mut self,
        _inbound_session_id: streamed_data::InboundSessionId,
    ) {
        self.insert_inbound_session_id_to_waiting_list_call_count += 1;
    }

    fn close_outbound_session(&mut self, _outbound_session_id: OutboundSessionId) {
        self.close_outbound_session_call_count += 1;
    }

    fn fetch_pending_header_for_session(
        &mut self,
        outbound_session_id: OutboundSessionId,
    ) -> Result<super::BlockHeader, super::behaviour::BehaviourInternalError> {
        self.fetch_pending_header_for_session_call_count += 1;
        if let Some(header) = self.header_pending_pairing.remove(&outbound_session_id) {
            Ok(header.try_into().unwrap())
        } else {
            Err(super::behaviour::BehaviourInternalError::HeaderNotFound)
        }
    }

    fn store_header_pending_pairing_with_signature(
        &mut self,
        header: protobuf::BlockHeader,
        outbound_session_id: OutboundSessionId,
    ) -> Result<(), super::behaviour::BehaviourInternalError> {
        self.store_header_pending_pairing_with_signature_call_count += 1;
        self.header_pending_pairing.insert(outbound_session_id, header.clone());
        Ok(())
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
