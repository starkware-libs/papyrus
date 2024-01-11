use std::collections::{HashMap, HashSet};
use std::task::{Context, Poll};

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

use super::{BlockHeader, BlockHeaderData, Event, SessionError};
use crate::db_executor::Data;
use crate::messages::protobuf;
use crate::streamed_data::behaviour::Event as StreamedDataEvent;
use crate::streamed_data::{self, Config, InboundSessionId, OutboundSessionId, SessionId};
use crate::BlockQuery;

#[cfg(test)]
#[path = "behaviour_test.rs"]
mod behaviour_test;
#[cfg(test)]
#[path = "flow_test.rs"]
mod flow_test;

pub(crate) struct Behaviour {
    streamed_data_behaviour: streamed_data::behaviour::Behaviour<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
    >,
    header_pending_pairing: HashMap<OutboundSessionId, protobuf::BlockHeader>,
    outbound_sessions_pending_termination: HashSet<OutboundSessionId>,
    inbound_sessions_pending_termination: HashSet<InboundSessionId>,
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub(crate) struct SessionIdNotFoundError(
    #[from] crate::streamed_data::behaviour::SessionIdNotFoundError,
);

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub(crate) struct PeerNotConnected(#[from] crate::streamed_data::behaviour::PeerNotConnected);

impl Behaviour {
    #[allow(dead_code)]
    pub fn new(config: Config) -> Self {
        Self {
            streamed_data_behaviour: streamed_data::behaviour::Behaviour::new(config),
            header_pending_pairing: HashMap::new(),
            outbound_sessions_pending_termination: HashSet::new(),
            inbound_sessions_pending_termination: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    pub fn send_query(
        &mut self,
        query: BlockQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.streamed_data_behaviour.send_query(query.into(), peer_id).map_err(|e| e.into())
    }

    /// Send data to the session that is mapped to this query id.
    /// return false if the query id is not mapped to any session.
    #[allow(dead_code)]
    pub fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let header_messages = match data {
            Data::BlockHeaderAndSignature { header, signature } => {
                vec![
                    protobuf::block_headers_response_part::HeaderMessage::Header(
                        header.clone().into(),
                    ),
                    protobuf::block_headers_response_part::HeaderMessage::Signatures(
                        protobuf::Signatures {
                            signatures: vec![signature.into()],
                            block: Some(protobuf::BlockId {
                                number: header.block_number.0,
                                header: Some(header.block_hash.into()),
                            }),
                        },
                    ),
                ]
            }
            Data::Fin { .. } => {
                // TODO: handle different Fin messages
                vec![protobuf::block_headers_response_part::HeaderMessage::Fin(Default::default())]
            }
        };
        for header_message in header_messages {
            self.streamed_data_behaviour
                .send_data(
                    protobuf::BlockHeadersResponse {
                        part: vec![protobuf::BlockHeadersResponsePart {
                            header_message: Some(header_message.clone()),
                        }],
                    },
                    inbound_session_id,
                )
                .or(Err(SessionIdNotFoundError(
                    streamed_data::behaviour::SessionIdNotFoundError {},
                )))?;
            if let protobuf::block_headers_response_part::HeaderMessage::Fin { .. } = header_message
            {
                // TODO: consider removing fin as a user sent mesages and have the user call
                // close_inbound_session instead.
                self.close_inbound_session(inbound_session_id);
            }
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
}

// functionality moved into this trait so that we can mock it in tests.
trait BehaviourTrait {
    fn store_header_pending_pairing_with_signature(
        &mut self,
        header: protobuf::BlockHeader,
        outbound_session_id: OutboundSessionId,
    ) -> Result<(), SessionError>;

    fn fetch_header_pending_pairing_with_signature(
        &mut self,
        outbound_session_id: OutboundSessionId,
    ) -> Option<BlockHeader>;

    fn close_outbound_session(&mut self, outbound_session_id: OutboundSessionId);

    fn handle_received_data(
        &mut self,
        data: protobuf::BlockHeadersResponse,
        outbound_session_id: OutboundSessionId,
    ) -> Option<Event> {
        // TODO: handle getting more then one message part in the response.
        if let Some(message) = data.part.first().and_then(|part| part.header_message.clone()) {
            match message {
                // TODO: consider if two consecutive headers is an error or not and what it the
                // right way to handle it.
                protobuf::block_headers_response_part::HeaderMessage::Header(header) => self
                    .store_header_pending_pairing_with_signature(header, outbound_session_id)
                    .err()
                    .map(|e| Event::SessionFailed {
                        session_id: outbound_session_id.into(),
                        session_error: e,
                    }),
                protobuf::block_headers_response_part::HeaderMessage::Signatures(sigs) => {
                    let Some(block_header) =
                        self.fetch_header_pending_pairing_with_signature(outbound_session_id)
                    else {
                        return Some(Event::SessionFailed {
                            session_id: outbound_session_id.into(),
                            session_error: SessionError::PairingError,
                        });
                    };
                    let Some(signatures) = sigs.try_into().ok() else {
                        return Some(Event::SessionFailed {
                            session_id: outbound_session_id.into(),
                            session_error: SessionError::IncompatibleDataError,
                        });
                    };
                    Some(Event::ReceivedData {
                        data: BlockHeaderData { block_header, signatures },
                        outbound_session_id,
                    })
                }
                protobuf::block_headers_response_part::HeaderMessage::Fin(_) => {
                    self.close_outbound_session(outbound_session_id);
                    Some(Event::SessionFailed {
                        session_id: SessionId::OutboundSessionId(outbound_session_id),
                        session_error: SessionError::ReceivedFin,
                    })
                }
            }
        } else {
            Some(Event::SessionFailed {
                session_id: outbound_session_id.into(),
                session_error: SessionError::IncompatibleDataError,
            })
        }
    }

    fn handle_session_closed_by_request(&mut self, session_id: SessionId) -> Event;

    fn handle_outbound_session_closed_by_peer(
        &mut self,
        outbound_session_id: OutboundSessionId,
    ) -> Event;

    fn map_streamed_data_behaviour_event_to_own_event(
        &mut self,
        in_event: StreamedDataEvent<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>,
    ) -> Option<Event> {
        match in_event {
            StreamedDataEvent::NewInboundSession { query, inbound_session_id, peer_id: _ } => {
                let query = match query.try_into() {
                    Ok(query) => query,
                    Err(e) => return Some(Event::ProtobufConversionError(e)),
                };
                Some(Event::NewInboundQuery { query, inbound_session_id })
            }
            StreamedDataEvent::SessionFailed { session_id, error } => Some(Event::SessionFailed {
                session_id,
                session_error: SessionError::StreamedData(error),
            }),
            streamed_data::GenericEvent::SessionClosedByPeer { session_id } => {
                let SessionId::OutboundSessionId(outbound_session_id) = session_id else {
                    return Some(Event::SessionFailed {
                        session_id,
                        session_error: SessionError::IncorrectSessionId,
                    });
                };
                Some(self.handle_outbound_session_closed_by_peer(outbound_session_id))
            }
            streamed_data::GenericEvent::SessionClosedByRequest { session_id } => {
                Some(self.handle_session_closed_by_request(session_id))
            }
            StreamedDataEvent::ReceivedData { data, outbound_session_id } => {
                self.handle_received_data(data, outbound_session_id)
            }
        }
    }
}

impl BehaviourTrait for Behaviour {
    fn store_header_pending_pairing_with_signature(
        &mut self,
        header: protobuf::BlockHeader,
        outbound_session_id: OutboundSessionId,
    ) -> Result<(), SessionError> {
        self.header_pending_pairing
            .insert(outbound_session_id, header.clone())
            .map(|_| ())
            .xor(Some(()))
            .ok_or_else(|| SessionError::PairingError)
    }

    fn fetch_header_pending_pairing_with_signature(
        &mut self,
        outbound_session_id: OutboundSessionId,
    ) -> Option<BlockHeader> {
        self.header_pending_pairing
            .remove(&outbound_session_id)
            .map(|header| header.try_into())
            .and_then(|header| header.ok())
    }

    /// Instruct behaviour to close an outbound session. A corresponding event will be emitted when
    /// the session is closed.
    fn close_outbound_session(&mut self, outbound_session_id: OutboundSessionId) {
        // TODO: handle errors in this function
        let _newly_inserted =
            self.outbound_sessions_pending_termination.insert(outbound_session_id);
        let _ = self
            .streamed_data_behaviour
            .close_session(SessionId::OutboundSessionId(outbound_session_id));
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
}

impl NetworkBehaviour for Behaviour {
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
                    // Due to the use of "map_out" functionality of libp2p we must return an event
                    // from this function. Therefore in the case where we want
                    // to ignore the event and return a pending poll we mark it and return a dummy
                    // event.
                    if let Some(event) =
                        self.map_streamed_data_behaviour_event_to_own_event(streamed_data_event)
                    {
                        event
                    } else {
                        ignore_event_and_return_pending = true;
                        Event::SessionFailed {
                            session_id: OutboundSessionId::default().into(),
                            session_error: SessionError::WaitingToCompletePairing,
                        }
                    }
                });
                if ignore_event_and_return_pending { Poll::Pending } else { Poll::Ready(event) }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
