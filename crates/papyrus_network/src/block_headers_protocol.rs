#[cfg(test)]
#[path = "block_headers_protocol_test.rs"]
mod block_headers_protocol_test;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use crate::db_executor::{self, Data, QueryId};
use crate::messages::{protobuf, ProtobufConversionError};
use crate::streamed_data_protocol::{self, InboundSessionId, OutboundSessionId, SessionId};
use crate::{BlockHeader, BlockHeaderData, BlockQuery, Signature};

#[cfg_attr(test, derive(Debug))]
pub(crate) enum SessionError {
    Inner(streamed_data_protocol::behaviour::SessionError),
    InnerEventConversionError,
    BatchingError,
    SessionClosedUnexpectedly,
    WaitingToCompleteBatching,
    ReceivedFin,
    IncorrectSessionId,
}

#[cfg_attr(test, derive(Debug))]
#[allow(dead_code)]
pub(crate) enum Event {
    NewInboundQuery {
        query: BlockQuery,
        inbound_session_id: streamed_data_protocol::InboundSessionId,
    },
    RecievedData {
        data: BlockHeaderData,
        outbound_session_id: streamed_data_protocol::OutboundSessionId,
    },
    SessionFailed {
        session_id: SessionId,
        session_error: SessionError,
    },
    ProtobufConversionError(ProtobufConversionError),
    SessionCompletedSuccessfully {
        session_id: SessionId,
    },
}

#[allow(dead_code)]
pub(crate) struct Behaviour<DBExecutor>
where
    DBExecutor: db_executor::DBExecutor,
{
    // TODO: make this a trait of type "streamed_data_protocol::behaviour::BehaviourTrait" (new
    // trait to add) so that the test can mock the inner behaviour.
    inner_behaviour: streamed_data_protocol::behaviour::Behaviour<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
    >,
    data_pending_batching:
        HashMap<OutboundSessionId, protobuf::block_headers_response_part::HeaderMessage>,
    outbound_sessions_pending_termination: HashSet<OutboundSessionId>,
    inbound_sessions_pending_termination: HashSet<InboundSessionId>,
    db_executor: Arc<DBExecutor>,
    query_id_to_inbound_session_id: HashMap<QueryId, InboundSessionId>,
    inbound_session_ids_pending_query_id: HashSet<InboundSessionId>,
}

#[cfg_attr(test, derive(Debug))]
enum BehaviourInternalError {
    ProtobufConversionError(ProtobufConversionError),
    BatchingError,
}

pub(crate) trait BehaviourTrait {
    fn map_inner_behaviour_event_to_own_event(
        &mut self,
        in_event: streamed_data_protocol::behaviour::Event<
            protobuf::BlockHeadersRequest,
            protobuf::BlockHeadersResponse,
        >,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event;
}

impl<DBExecutor> Behaviour<DBExecutor>
where
    DBExecutor: db_executor::DBExecutor,
{
    #[allow(dead_code)]
    pub fn new(substream_timeout: Duration, db_executor: Arc<DBExecutor>) -> Self {
        Self {
            inner_behaviour: streamed_data_protocol::behaviour::Behaviour::new(substream_timeout),
            data_pending_batching: HashMap::new(),
            outbound_sessions_pending_termination: HashSet::new(),
            inbound_sessions_pending_termination: HashSet::new(),
            db_executor,
            query_id_to_inbound_session_id: HashMap::new(),
            inbound_session_ids_pending_query_id: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    pub fn send_query(&mut self, query: protobuf::BlockHeadersRequest, peer_id: PeerId) {
        let _outbound_session_id = self.inner_behaviour.send_query(query, peer_id);
    }

    /// Send data to the session that is mapped to this query id.
    /// return false if the query id is not mapped to any session.
    #[allow(dead_code)]
    pub fn send_data(&mut self, data: Data, query_id: QueryId) -> bool {
        let inbound_session_id = self.query_id_to_inbound_session_id.get(&query_id);
        if let Some(inbound_session_id) = inbound_session_id {
            let (header_message, _block_number) = match data {
                Data::BlockHeader(block_header) => {
                    let block_number = block_header.block_number.0;
                    (
                        protobuf::block_headers_response_part::HeaderMessage::Header(
                            block_header.into(),
                        ),
                        block_number,
                    )
                }
                Data::Fin { block_number } => (
                    protobuf::block_headers_response_part::HeaderMessage::Fin(Default::default()),
                    block_number,
                ),
            };
            // TODO: after this header should go its signatures.
            let Ok(_) = self.inner_behaviour.send_data(
                protobuf::BlockHeadersResponse {
                    part: vec![protobuf::BlockHeadersResponsePart {
                        header_message: Some(header_message.clone()),
                    }],
                },
                *inbound_session_id,
            ) else {
                return false;
            };
            if let protobuf::block_headers_response_part::HeaderMessage::Fin { .. } = header_message
            {
                // remove the session id from the mapping here since we need the query id to find it
                // in the hash map.
                #[allow(clippy::clone_on_copy)]
                // we need to clone here since we need self.query_id_to_inbound_session_id to be
                // mutable.
                let inbound_session_id = inbound_session_id.clone();
                self.query_id_to_inbound_session_id.remove(&query_id);
                self.close_inbound_session(inbound_session_id);
            }
            true
        } else {
            false
        }
    }

    /// Instruct behaviour to close an inbound session. A corresponding event will be emitted to
    /// report the session was closed.
    #[allow(dead_code)]
    pub fn close_inbound_session(&mut self, inbound_session_id: InboundSessionId) {
        let _newly_inserted = self.inbound_sessions_pending_termination.insert(inbound_session_id);
        let _ = self.inner_behaviour.close_session(SessionId::InboundSessionId(inbound_session_id));
    }

    /// Instruct behaviour to close an outbound session. A corresponding event will be emitted when
    /// the session is closed.
    #[allow(dead_code)]
    pub fn close_outbound_session(&mut self, outbound_session_id: OutboundSessionId) {
        let _newly_inserted =
            self.outbound_sessions_pending_termination.insert(outbound_session_id);
        let _ =
            self.inner_behaviour.close_session(SessionId::OutboundSessionId(outbound_session_id));
    }

    /// Adds the query id to the map of inbound session id to query id.
    /// returns true if the inbound session id was pending to be matched with a query id and was not
    /// in the map before.
    pub fn register_query_id(
        &mut self,
        query_id: QueryId,
        inbound_session_id: InboundSessionId,
    ) -> bool {
        let removed = self.inbound_session_ids_pending_query_id.remove(&inbound_session_id);
        let old_value = self.query_id_to_inbound_session_id.insert(query_id, inbound_session_id);
        removed && old_value.is_none()
    }

    // tries to covert the protobuf message to block header or signatures.
    // if the message is of type fin it will panic.
    //
    fn header_message_to_header_or_signatures(
        &self,
        header_message: &protobuf::block_headers_response_part::HeaderMessage,
    ) -> Result<(Option<BlockHeader>, Option<Vec<Signature>>), BehaviourInternalError> {
        match header_message {
            protobuf::block_headers_response_part::HeaderMessage::Header(header) => {
                match header.clone().try_into() {
                    Ok(header) => Ok((Some(header), None)),
                    Err(e) => Err(BehaviourInternalError::ProtobufConversionError(e)),
                }
            }
            protobuf::block_headers_response_part::HeaderMessage::Signatures(sigs) => {
                let mut signatures = Vec::new();
                // TODO: is empty sigs vector a valid message?
                for sig in &sigs.signatures {
                    match sig.clone().try_into() {
                        Ok(sig) => signatures.push(sig),
                        Err(e) => return Err(BehaviourInternalError::ProtobufConversionError(e)),
                    }
                }
                Ok((None, Some(signatures)))
            }
            protobuf::block_headers_response_part::HeaderMessage::Fin(_) => unreachable!(),
        }
    }

    // this function assumes that the data and header_message are each one of block header or
    // signatures but not the same one. the function will return error if both  parameter will
    // evaluate to none or the same type.
    fn get_block_header_and_signatures_from_event_and_stored_data(
        &self,
        data: &protobuf::block_headers_response_part::HeaderMessage,
        header_message: &protobuf::block_headers_response_part::HeaderMessage,
    ) -> Result<(BlockHeader, Vec<Signature>), BehaviourInternalError> {
        let (block_header_x, signatures_x) = self.header_message_to_header_or_signatures(data)?;
        let (block_header_y, signatures_y) =
            self.header_message_to_header_or_signatures(header_message)?;
        let Some(block_header) = block_header_x.or_else(|| block_header_y.or(None)) else {
            return Err(BehaviourInternalError::BatchingError {});
        };
        let Some(signatures) = signatures_x.or_else(|| signatures_y.or(None)) else {
            return Err(BehaviourInternalError::BatchingError {});
        };
        Ok((block_header, signatures))
    }

    fn handle_batching(
        &mut self,
        outbound_session_id: OutboundSessionId,
        header_message: &protobuf::block_headers_response_part::HeaderMessage,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event {
        if let Some(data) = self.data_pending_batching.get(&outbound_session_id) {
            *ignore_event_and_return_pending = false;
            match self
                .get_block_header_and_signatures_from_event_and_stored_data(data, header_message)
            {
                Ok((block_header, signatures)) => Event::RecievedData {
                    data: BlockHeaderData { block_header, signatures },
                    outbound_session_id,
                },
                Err(e) => match e {
                    BehaviourInternalError::ProtobufConversionError(e) => {
                        Event::ProtobufConversionError(e)
                    }
                    BehaviourInternalError::BatchingError => Event::SessionFailed {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
                        session_error: SessionError::BatchingError,
                    },
                },
            }
        } else {
            *ignore_event_and_return_pending = true;
            self.data_pending_batching.insert(outbound_session_id, header_message.clone());
            Event::SessionFailed {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
                session_error: SessionError::WaitingToCompleteBatching,
            }
        }
    }

    fn handle_received_data(
        &mut self,
        data: protobuf::BlockHeadersResponse,
        outbound_session_id: OutboundSessionId,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event {
        // TODO: handle getting more then one message part in the response.
        if let Some(header_message) = data.part.first().and_then(|part| part.header_message.clone())
        {
            match header_message {
                protobuf::block_headers_response_part::HeaderMessage::Header(_)
                | protobuf::block_headers_response_part::HeaderMessage::Signatures(_) => self
                    .handle_batching(
                        outbound_session_id,
                        &header_message,
                        ignore_event_and_return_pending,
                    ),
                protobuf::block_headers_response_part::HeaderMessage::Fin(_) => {
                    *ignore_event_and_return_pending = true;
                    self.close_outbound_session(outbound_session_id);
                    Event::SessionFailed {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
                        session_error: SessionError::ReceivedFin,
                    }
                }
            }
        } else {
            Event::SessionFailed {
                session_id: SessionId::OutboundSessionId(OutboundSessionId { value: usize::MAX }),
                session_error: SessionError::InnerEventConversionError,
            }
        }
    }

    fn handle_session_closed_by_request(&mut self, session_id: SessionId) -> Event {
        // TODO: improve error handling when this unexpected case happens
        match session_id {
            SessionId::OutboundSessionId(outbound_session_id) => {
                if self.outbound_sessions_pending_termination.remove(&outbound_session_id) {
                    Event::SessionCompletedSuccessfully {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
                    }
                } else {
                    Event::SessionFailed {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
                        session_error: SessionError::SessionClosedUnexpectedly,
                    }
                }
            }
            SessionId::InboundSessionId(inbound_session_id) => {
                if self.inbound_sessions_pending_termination.remove(&inbound_session_id) {
                    Event::SessionCompletedSuccessfully {
                        session_id: SessionId::InboundSessionId(inbound_session_id),
                    }
                } else {
                    Event::SessionFailed {
                        session_id: SessionId::InboundSessionId(inbound_session_id),
                        session_error: SessionError::SessionClosedUnexpectedly,
                    }
                }
            }
        }
    }

    fn handle_outbound_session_closed_by_peer(
        &mut self,
        outbound_session_id: OutboundSessionId,
    ) -> Event {
        if self.outbound_sessions_pending_termination.remove(&outbound_session_id) {
            Event::SessionCompletedSuccessfully {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
            }
        } else {
            Event::SessionFailed {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
                session_error: SessionError::SessionClosedUnexpectedly,
            }
        }
    }

    fn handle_new_inbound_session(
        &mut self,
        query: protobuf::BlockHeadersRequest,
        inbound_session_id: InboundSessionId,
    ) -> Event {
        let query = query.try_into().unwrap();
        let newly_inserted = self.inbound_session_ids_pending_query_id.insert(inbound_session_id);
        // TODO: should we assert that the value did not exist before? the session id should
        // be unique so we shouldn't have one before.
        assert!(newly_inserted);
        Event::NewInboundQuery { query, inbound_session_id }
    }
}

impl<DBExecutor> BehaviourTrait for Behaviour<DBExecutor>
where
    DBExecutor: db_executor::DBExecutor,
{
    fn map_inner_behaviour_event_to_own_event(
        &mut self,
        in_event: streamed_data_protocol::behaviour::Event<
            protobuf::BlockHeadersRequest,
            protobuf::BlockHeadersResponse,
        >,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event {
        match in_event {
            streamed_data_protocol::behaviour::Event::NewInboundSession {
                query,
                inbound_session_id,
                peer_id: _,
            } => self.handle_new_inbound_session(query, inbound_session_id),
            streamed_data_protocol::behaviour::Event::SessionFailed { session_id, error } => {
                Event::SessionFailed { session_id, session_error: SessionError::Inner(error) }
            }
            streamed_data_protocol::GenericEvent::SessionClosedByPeer { session_id } => {
                let SessionId::OutboundSessionId(outbound_session_id) = session_id else {
                    return Event::SessionFailed {
                        session_id,
                        session_error: SessionError::IncorrectSessionId,
                    };
                };
                self.handle_outbound_session_closed_by_peer(outbound_session_id)
            }
            streamed_data_protocol::GenericEvent::SessionClosedByRequest { session_id } => {
                self.handle_session_closed_by_request(session_id)
            }
            streamed_data_protocol::behaviour::Event::ReceivedData {
                data,
                outbound_session_id,
            } => self.handle_received_data(
                data,
                outbound_session_id,
                ignore_event_and_return_pending,
            ),
        }
    }
}

impl<DBExecutor> NetworkBehaviour for Behaviour<DBExecutor>
where
    // DBExecutor must have static lifetime
    // since NetworkBehaviour requires it.
    DBExecutor: db_executor::DBExecutor + 'static,
{
    type ConnectionHandler = streamed_data_protocol::handler::Handler<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
    >;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.inner_behaviour.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.inner_behaviour.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.inner_behaviour.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        self.inner_behaviour.on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        match self.inner_behaviour.poll(cx) {
            Poll::Ready(inner_event) => {
                let mut ignore_event_and_return_pending = false;
                let event = inner_event.map_out(|inner_event| {
                    self.map_inner_behaviour_event_to_own_event(
                        inner_event,
                        &mut ignore_event_and_return_pending,
                    )
                });
                if ignore_event_and_return_pending { Poll::Pending } else { Poll::Ready(event) }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
