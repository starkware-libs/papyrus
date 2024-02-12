use std::collections::HashSet;
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
use libp2p::{Multiaddr, PeerId, StreamProtocol};

use super::{Event, SessionError};
use crate::db_executor::Data;
use crate::messages::{protobuf, ProtobufConversionError};
use crate::streamed_data::behaviour::Event as StreamedDataEvent;
use crate::streamed_data::{self, Config, InboundSessionId, OutboundSessionId, SessionId};
use crate::{InternalQuery};

#[cfg(test)]
#[path = "behaviour_test.rs"]
mod behaviour_test;

#[cfg(test)]
#[path = "flow_test.rs"]
mod flow_test;

const PROTOCOL_NAME: &str = "/starknet/headers/1";

pub(crate) struct Behaviour {
    streamed_data_behaviour: streamed_data::behaviour::Behaviour<
        protobuf::BlockHeadersRequest,
        protobuf::BlockHeadersResponse,
    >,
    sessions_pending_termination: HashSet<SessionId>,
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct SessionIdNotFoundError(#[from] crate::streamed_data::behaviour::SessionIdNotFoundError);

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct PeerNotConnected(#[from] crate::streamed_data::behaviour::PeerNotConnected);

impl Behaviour {
    pub fn new(session_timeout: Duration) -> Self {
        Self {
            streamed_data_behaviour: streamed_data::behaviour::Behaviour::new(Config {
                session_timeout,
                protocol_name: StreamProtocol::new(PROTOCOL_NAME),
            }),
            sessions_pending_termination: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn send_query(
        &mut self,
        query: InternalQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.streamed_data_behaviour.send_query(query.into(), peer_id).map_err(|e| e.into())
    }

    /// Send data to the session with the given id. Return Error if no such session exists.
    pub(crate) fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        let protobuf_data = match data {
            Data::BlockHeaderAndSignature { header, signature } => {
                protobuf::block_headers_response::HeaderMessage::Header((header, signature).into())
            }
            Data::Fin {} => protobuf::block_headers_response::HeaderMessage::Fin(protobuf::Fin {}),
        };
        let is_fin = matches!(
            protobuf_data,
            protobuf::block_headers_response::HeaderMessage::Fin(protobuf::Fin {})
        );
        self.streamed_data_behaviour.send_data(
            protobuf::BlockHeadersResponse { header_message: Some(protobuf_data) },
            inbound_session_id,
        )?;
        if is_fin {
            // TODO: consider removing fin as a user sent mesages and have the user call
            // close_inbound_session instead.
            self.close_inbound_session(inbound_session_id);
        }
        Ok(())
    }

    // TODO(shahak): Consider making this function private.
    /// Instruct behaviour to close an inbound session. A corresponding event will be emitted to
    /// report the session was closed.
    #[allow(dead_code)]
    pub fn close_inbound_session(&mut self, inbound_session_id: InboundSessionId) {
        self.sessions_pending_termination.insert(inbound_session_id.into());
        // TODO(shahak): handle error.
        let _ = self.streamed_data_behaviour.close_inbound_session(inbound_session_id);
    }
}

// functionality moved into this trait so that we can mock it in tests.
trait BehaviourTrait {
    fn handle_session_finished(&mut self, session_id: SessionId) -> Option<Event>;

    fn drop_session(&mut self, session_id: SessionId);

    fn get_sessions_pending_termination(&mut self) -> &mut HashSet<SessionId>;

    fn handle_received_data(
        &mut self,
        data: protobuf::BlockHeadersResponse,
        outbound_session_id: OutboundSessionId,
    ) -> Option<Event> {
        if self.get_sessions_pending_termination().contains(&outbound_session_id.into()) {
            self.drop_session(outbound_session_id.into());
            return Some(Event::SessionFailed {
                session_id: outbound_session_id.into(),
                session_error: SessionError::ReceivedMessageAfterFin,
            });
        }
        let Some(message) = data.header_message else {
            self.drop_session(outbound_session_id.into());
            return Some(Event::SessionFailed {
                session_id: outbound_session_id.into(),
                session_error: SessionError::ProtobufConversionError(
                    ProtobufConversionError::MissingField,
                ),
            });
        };
        match message {
            protobuf::block_headers_response::HeaderMessage::Header(header_with_signatures) => {
                match header_with_signatures.try_into() {
                    Ok(signed_header) => {
                        Some(Event::ReceivedData { signed_header, outbound_session_id })
                    }
                    Err(protobuf_conversion_error) => {
                        self.drop_session(outbound_session_id.into());
                        Some(Event::SessionFailed {
                            session_id: outbound_session_id.into(),
                            session_error: SessionError::ProtobufConversionError(
                                protobuf_conversion_error,
                            ),
                        })
                    }
                }
            }
            protobuf::block_headers_response::HeaderMessage::Fin(protobuf::Fin {}) => {
                let is_newly_inserted =
                    self.get_sessions_pending_termination().insert(outbound_session_id.into());
                if is_newly_inserted {
                    None
                } else {
                    self.drop_session(outbound_session_id.into());
                    Some(Event::SessionFailed {
                        session_id: outbound_session_id.into(),
                        session_error: SessionError::ReceivedMessageAfterFin,
                    })
                }
            }
        }
    }

    fn map_streamed_data_behaviour_event_to_own_event(
        &mut self,
        in_event: StreamedDataEvent<protobuf::BlockHeadersRequest, protobuf::BlockHeadersResponse>,
    ) -> Option<Event> {
        match in_event {
            StreamedDataEvent::NewInboundSession { query, inbound_session_id, peer_id: _ } => {
                match query.try_into() {
                    Ok(query) => Some(Event::NewInboundQuery { query, inbound_session_id }),
                    Err(e) => {
                        self.drop_session(inbound_session_id.into());
                        Some(Event::QueryConversionError(e))
                    }
                }
            }
            StreamedDataEvent::SessionFailed { session_id, error } => Some(Event::SessionFailed {
                session_id,
                session_error: SessionError::StreamedData(error),
            }),
            streamed_data::GenericEvent::SessionFinishedSuccessfully { session_id } => {
                self.handle_session_finished(session_id)
            }
            StreamedDataEvent::ReceivedData { data, outbound_session_id } => {
                self.handle_received_data(data, outbound_session_id)
            }
        }
    }
}

impl BehaviourTrait for Behaviour {
    fn handle_session_finished(&mut self, session_id: SessionId) -> Option<Event> {
        if self.sessions_pending_termination.remove(&session_id) {
            Some(Event::SessionFinishedSuccessfully { session_id })
        } else {
            Some(Event::SessionFailed {
                session_id,
                session_error: SessionError::SessionClosedUnexpectedly,
            })
        }
    }

    fn drop_session(&mut self, session_id: SessionId) {
        // Ignoring errors if they occur because an error here means the session doesn't exist, and
        // if the session doesn't exist we don't need to drop it.
        let _ = self.streamed_data_behaviour.drop_session(session_id);
    }

    fn get_sessions_pending_termination(&mut self) -> &mut HashSet<SessionId> {
        &mut self.sessions_pending_termination
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
                            session_error: SessionError::SessionClosedUnexpectedly,
                        }
                    }
                });
                if ignore_event_and_return_pending { Poll::Pending } else { Poll::Ready(event) }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
