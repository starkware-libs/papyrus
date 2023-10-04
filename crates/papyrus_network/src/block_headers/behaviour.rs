use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::task::{Context, Poll};

use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionHandler, ConnectionId, FromSwarm, NetworkBehaviour, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};

use super::{BlockHeader, BlockHeaderData, Event, SessionError, Signature};
use crate::db_executor::{self, Data, QueryId};
use crate::messages::{protobuf, ProtobufConversionError};
use crate::streamed_data::behaviour::Event as StreamedDataEvent;
use crate::streamed_data::{self, Config, InboundSessionId, OutboundSessionId, SessionId};

#[allow(dead_code)]
pub(crate) struct Behaviour<DBExecutor>
where
    DBExecutor: db_executor::DBExecutor,
{
    // TODO: make this a trait of type "streamed_data_protocol::behaviour::BehaviourTrait" (new
    // trait to add) so that the test can mock the streamed_data behaviour.
    streamed_data_behaviour: streamed_data::behaviour::Behaviour<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
    >,
    data_pending_pairing:
        HashMap<OutboundSessionId, protobuf::block_headers_response_part::HeaderMessage>,
    outbound_sessions_pending_termination: HashSet<OutboundSessionId>,
    inbound_sessions_pending_termination: HashSet<InboundSessionId>,
    db_executor: Arc<DBExecutor>,
    query_id_to_inbound_session_id: HashMap<QueryId, InboundSessionId>,
    inbound_session_ids_pending_query_id: HashSet<InboundSessionId>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BehaviourInternalError {
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    #[error("Pairing block header and signature error")]
    PairingError,
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub(crate) struct SessionIdNotFoundError(
    #[from] crate::streamed_data::behaviour::SessionIdNotFoundError,
);

pub(crate) trait BehaviourTrait {
    fn map_streamed_data_behaviour_event_to_own_event(
        &mut self,
        in_event: StreamedDataEvent<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event;
}

impl<DBExecutor> Behaviour<DBExecutor>
where
    DBExecutor: db_executor::DBExecutor,
{
    #[allow(dead_code)]
    pub fn new(config: Config, db_executor: Arc<DBExecutor>) -> Self {
        Self {
            streamed_data_behaviour: streamed_data::behaviour::Behaviour::new(config),
            data_pending_pairing: HashMap::new(),
            outbound_sessions_pending_termination: HashSet::new(),
            inbound_sessions_pending_termination: HashSet::new(),
            db_executor,
            query_id_to_inbound_session_id: HashMap::new(),
            inbound_session_ids_pending_query_id: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    pub fn send_query(&mut self, query: protobuf::BlockHeadersRequest, peer_id: PeerId) {
        // TODO: keep track of the query id and the session id so that we can map between them for reputation.
        let _outbound_session_id = self.streamed_data_behaviour.send_query(query, peer_id);
    }

    /// Send data to the session that is mapped to this query id.
    /// return false if the query id is not mapped to any session.
    #[allow(dead_code)]
    pub fn send_data(
        &mut self,
        data: Data,
        query_id: QueryId,
    ) -> Result<(), SessionIdNotFoundError> {
        let inbound_session_id = self
            .query_id_to_inbound_session_id
            .get(&query_id)
            .ok_or(SessionIdNotFoundError(streamed_data::behaviour::SessionIdNotFoundError {}))?;
        let header_message = match data {
            Data::BlockHeader(block_header) => {
                protobuf::block_headers_response_part::HeaderMessage::Header(block_header.into())
            }
            Data::Fin { .. } => {
                protobuf::block_headers_response_part::HeaderMessage::Fin(Default::default())
            }
        };
        // TODO: after this header should go its signatures.
        self.streamed_data_behaviour
            .send_data(
                protobuf::BlockHeadersResponse {
                    part: vec![protobuf::BlockHeadersResponsePart {
                        header_message: Some(header_message.clone()),
                    }],
                },
                *inbound_session_id,
            )
            .map_err(|e| SessionIdNotFoundError(e))?;
        if let protobuf::block_headers_response_part::HeaderMessage::Fin { .. } = header_message {
            // remove the session id from the mapping here since we need the query id to find it
            // in the hash map.
            #[allow(clippy::clone_on_copy)]
            // we need to clone here since we need self.query_id_to_inbound_session_id to be
            // mutable.
            let inbound_session_id = inbound_session_id.clone();
            self.query_id_to_inbound_session_id.remove(&query_id);
            self.close_inbound_session(inbound_session_id);
        }
        Ok(())
    }

    /// Instruct behaviour to close an inbound session. A corresponding event will be emitted to
    /// report the session was closed.
    #[allow(dead_code)]
    pub fn close_inbound_session(&mut self, inbound_session_id: InboundSessionId) {
        let _newly_inserted = self.inbound_sessions_pending_termination.insert(inbound_session_id);
        let _ = self
            .streamed_data_behaviour
            .close_session(SessionId::InboundSessionId(inbound_session_id));
    }

    /// Instruct behaviour to close an outbound session. A corresponding event will be emitted when
    /// the session is closed.
    #[allow(dead_code)]
    pub fn close_outbound_session(&mut self, outbound_session_id: OutboundSessionId) {
        let _newly_inserted =
            self.outbound_sessions_pending_termination.insert(outbound_session_id);
        let _ = self
            .streamed_data_behaviour
            .close_session(SessionId::OutboundSessionId(outbound_session_id));
    }

    /// Adds the query id to the map of inbound session id to query id.
    /// returns true if the inbound session id was pending to be matched with a query id and was not
    /// in the map before.
    fn register_query_id(
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
    pub fn header_message_to_header_or_signatures(
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
    // signatures but not the same one. the function will return error if both parameter will
    // evaluate to none or the same type.
    fn get_block_header_and_signatures_from_event_data_and_stored_data(
        &self,
        data: &protobuf::block_headers_response_part::HeaderMessage,
        header_message: &protobuf::block_headers_response_part::HeaderMessage,
    ) -> Result<(BlockHeader, Vec<Signature>), BehaviourInternalError> {
        let (block_header_x, signatures_x) = self.header_message_to_header_or_signatures(data)?;
        let (block_header_y, signatures_y) =
            self.header_message_to_header_or_signatures(header_message)?;
        let Some(block_header) = block_header_x.or_else(|| block_header_y.or(None)) else {
            return Err(BehaviourInternalError::PairingError {});
        };
        let Some(signatures) = signatures_x.or_else(|| signatures_y.or(None)) else {
            return Err(BehaviourInternalError::PairingError {});
        };
        Ok((block_header, signatures))
    }

    pub(crate) fn handle_pairing_of_header_and_signature(
        &mut self,
        outbound_session_id: OutboundSessionId,
        header_message: &protobuf::block_headers_response_part::HeaderMessage,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event {
        if let Some(data) = self.data_pending_pairing.get(&outbound_session_id) {
            *ignore_event_and_return_pending = false;
            match self.get_block_header_and_signatures_from_event_data_and_stored_data(
                data,
                header_message,
            ) {
                Ok((block_header, signatures)) => Event::RecievedData {
                    data: BlockHeaderData { block_header, signatures },
                    outbound_session_id,
                },
                Err(e) => match e {
                    BehaviourInternalError::ProtobufConversionError(e) => {
                        Event::ProtobufConversionError(e)
                    }
                    BehaviourInternalError::PairingError => Event::SessionFailed {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
                        session_error: SessionError::PairingError,
                    },
                },
            }
        } else {
            *ignore_event_and_return_pending = true;
            self.data_pending_pairing.insert(outbound_session_id, header_message.clone());
            Event::SessionFailed {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
                session_error: SessionError::WaitingToCompletePairing,
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
                    .handle_pairing_of_header_and_signature(
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
                session_error: SessionError::StreamedDataEventConversionError,
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
        let query = match query.try_into() {
            Ok(query) => query,
            Err(e) => return Event::ProtobufConversionError(e),
        };
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
    fn map_streamed_data_behaviour_event_to_own_event(
        &mut self,
        in_event: StreamedDataEvent<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>,
        ignore_event_and_return_pending: &mut bool,
    ) -> Event {
        match in_event {
            StreamedDataEvent::NewInboundSession { query, inbound_session_id, peer_id: _ } => {
                self.handle_new_inbound_session(query, inbound_session_id)
            }
            StreamedDataEvent::SessionFailed { session_id, error } => Event::SessionFailed {
                session_id,
                session_error: SessionError::StreamedData(error),
            },
            streamed_data::GenericEvent::SessionClosedByPeer { session_id } => {
                let SessionId::OutboundSessionId(outbound_session_id) = session_id else {
                    return Event::SessionFailed {
                        session_id,
                        session_error: SessionError::IncorrectSessionId,
                    };
                };
                self.handle_outbound_session_closed_by_peer(outbound_session_id)
            }
            streamed_data::GenericEvent::SessionClosedByRequest { session_id } => {
                self.handle_session_closed_by_request(session_id)
            }
            StreamedDataEvent::ReceivedData { data, outbound_session_id } => self
                .handle_received_data(data, outbound_session_id, ignore_event_and_return_pending),
        }
    }
}

impl<DBExecutor> NetworkBehaviour for Behaviour<DBExecutor>
where
    // DBExecutor must have static lifetime
    // since NetworkBehaviour requires it.
    DBExecutor: db_executor::DBExecutor + 'static,
{
    type ConnectionHandler = streamed_data::handler::Handler<
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
        self.streamed_data_behaviour.handle_established_inbound_connection(
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
        self.streamed_data_behaviour.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.streamed_data_behaviour.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        self.streamed_data_behaviour.on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        match self.streamed_data_behaviour.poll(cx) {
            Poll::Ready(streamed_data_event) => {
                let mut ignore_event_and_return_pending = false;
                let event = streamed_data_event.map_out(|streamed_data_event| {
                    self.map_streamed_data_behaviour_event_to_own_event(
                        streamed_data_event,
                        &mut ignore_event_and_return_pending,
                    )
                });
                if ignore_event_and_return_pending {
                    Poll::Pending
                } else {
                    Poll::Ready(event)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}