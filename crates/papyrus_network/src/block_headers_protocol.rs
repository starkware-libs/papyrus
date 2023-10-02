#[cfg(test)]
#[path = "block_headers_protocol_test.rs"]
mod block_headers_protocol_test;

use std::{
    collections::{HashMap, HashSet},
    task::{Context, Poll},
    time::Duration,
};

use libp2p::{
    core::Endpoint,
    swarm::{
        ConnectionDenied, ConnectionHandler, ConnectionId, FromSwarm, NetworkBehaviour,
        PollParameters, ToSwarm,
    },
    Multiaddr, PeerId,
};

use crate::{
    messages::{
        block::{BlockHeadersRequest, BlockHeadersResponse},
        common::ProtobufConversionError,
        proto::p2p::proto::block_headers_response::HeaderMessage,
    },
    streamed_data_protocol::{self, OutboundSessionId, SessionId},
    BlockHeader, BlockHeaderData, BlockQuery, Signature,
};

#[cfg_attr(test, derive(Debug))]
pub(crate) enum SessionError {
    InnerSessionError(streamed_data_protocol::behaviour::SessionError),
    InnerEventConversionError,
    WaitingToCompleteBatching,
    BatchingError,
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
        outbound_session_id: streamed_data_protocol::OutboundSessionId,
    },
}

#[allow(dead_code)]
pub(crate) struct Behaviour {
    // TODO: make this a trait of type "streamed_data_protocol::behaviour::BehaviourTrait" (new trait to add) so that the test can mock the inner behaviour.
    inner_behaviour:
        streamed_data_protocol::behaviour::Behaviour<BlockHeadersRequest, BlockHeadersResponse>,
    data_pending_batching: HashMap<OutboundSessionId, HeaderMessage>,
    outbound_sessions_pending_termination: HashSet<OutboundSessionId>,
}

#[cfg_attr(test, derive(Debug))]
enum BehaviourInternalError {
    ProtobufConversionError(ProtobufConversionError),
    BatchingError,
}

trait BehaviourTrait {
    fn map_generic_behaviour_event_to_specific_event(
        &mut self,
        in_event: streamed_data_protocol::behaviour::Event<
            BlockHeadersRequest,
            BlockHeadersResponse,
        >,
        wait_to_complete_batching: &mut bool,
    ) -> Event;
}

impl Behaviour {
    #[allow(dead_code)]
    fn new(substream_timeout: Duration) -> Self {
        Self {
            inner_behaviour: streamed_data_protocol::behaviour::Behaviour::new(substream_timeout),
            data_pending_batching: HashMap::new(),
            outbound_sessions_pending_termination: HashSet::new(),
        }
    }

    // tries to covert the protobuf message to block header or signatures.
    // if the message is of type fin it will panic.
    //
    fn header_message_to_header_or_signatures(
        &self,
        header_message: &HeaderMessage,
    ) -> Result<(Option<BlockHeader>, Option<Vec<Signature>>), BehaviourInternalError> {
        match header_message {
            HeaderMessage::Header(header) => match header.clone().try_into() {
                Ok(header) => Ok((Some(header), None)),
                Err(e) => return Err(BehaviourInternalError::ProtobufConversionError(e)),
            },
            HeaderMessage::Signatures(sigs) => {
                let mut signatures = Vec::new();
                //TODO: is empty sigs vector a valid message?
                for sig in &sigs.signatures {
                    match sig.clone().try_into() {
                        Ok(sig) => signatures.push(sig),
                        Err(e) => return Err(BehaviourInternalError::ProtobufConversionError(e)),
                    }
                }
                Ok((None, Some(signatures)))
            }
            HeaderMessage::Fin(_) => unreachable!(),
        }
    }

    // this function assumes that the data and header_message are each one of block header or signatures but not the same one.
    // the function will return error if both  parameter will evaluate to none or the same type.
    fn get_block_header_and_signatures_from_event_and_stored_data(
        &self,
        data: &HeaderMessage,
        header_message: &HeaderMessage,
    ) -> Result<(BlockHeader, Vec<Signature>), BehaviourInternalError> {
        let (block_header_x, signatures_x) = self.header_message_to_header_or_signatures(data)?;
        let (block_header_y, signatures_y) =
            self.header_message_to_header_or_signatures(&header_message)?;
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
        header_message: &HeaderMessage,
        wait_to_complete_batching: &mut bool,
    ) -> Event {
        if let Some(data) = self.data_pending_batching.get(&outbound_session_id) {
            *wait_to_complete_batching = false;
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
            *wait_to_complete_batching = true;
            self.data_pending_batching.insert(outbound_session_id, header_message.clone());
            Event::SessionFailed {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
                session_error: SessionError::WaitingToCompleteBatching,
            }
        }
    }

    fn handle_fin(&mut self, outbound_session_id: OutboundSessionId) -> Event {
        self.inner_behaviour.closed_outbound_session(outbound_session_id);
        // TODO: uncomment this when we have the connection closed event from the inner behaviour and then we can insert here and pop when we get the event.
        // let _newly_inserted =
        //     self.outbound_sessions_pending_termination.insert(outbound_session_id);

        //TODO: check if the session closed successfully or not.
        Event::SessionCompletedSuccessfully { outbound_session_id }
    }
}

impl BehaviourTrait for Behaviour {
    fn map_generic_behaviour_event_to_specific_event(
        &mut self,
        in_event: streamed_data_protocol::behaviour::Event<
            BlockHeadersRequest,
            BlockHeadersResponse,
        >,
        wait_to_complete_batching: &mut bool,
    ) -> Event {
        match in_event {
            streamed_data_protocol::behaviour::Event::NewInboundSession {
                query,
                inbound_session_id,
            } => {
                let query = query.try_into().unwrap();
                Event::NewInboundQuery { query, inbound_session_id }
            }
            streamed_data_protocol::behaviour::Event::SessionFailed { session_id, error } => {
                Event::SessionFailed {
                    session_id,
                    session_error: SessionError::InnerSessionError(error),
                }
            }
            streamed_data_protocol::GenericEvent::OutboundSessionClosedByPeer {
                outbound_session_id,
            } => {
                unimplemented!()
            }
            streamed_data_protocol::GenericEvent::SessionClosedByRequest { session_id } => {
                unimplemented!()
            }
            streamed_data_protocol::behaviour::Event::ReceivedData {
                data,
                outbound_session_id,
            } => {
                if let Some(header_message) = data.header_message {
                    match header_message {
                        HeaderMessage::Header(_) | HeaderMessage::Signatures(_) => self
                            .handle_batching(
                                outbound_session_id,
                                &header_message,
                                wait_to_complete_batching,
                            ),
                        HeaderMessage::Fin(_) => self.handle_fin(outbound_session_id),
                    }
                } else {
                    Event::SessionFailed {
                        session_id: SessionId::OutboundSessionId(OutboundSessionId {
                            value: usize::MAX,
                        }),
                        session_error: SessionError::InnerEventConversionError,
                    }
                }
            }
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler =
        streamed_data_protocol::handler::Handler<BlockHeadersRequest, BlockHeadersResponse>;
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

    fn on_swarm_event(&mut self, event: FromSwarm<'_, Self::ConnectionHandler>) {
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
        params: &mut impl PollParameters,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        match self.inner_behaviour.poll(cx, params) {
            Poll::Ready(inner_event) => {
                let mut wait_to_complete_batching = false;
                let event = inner_event.map_out(|inner_event| {
                    self.map_generic_behaviour_event_to_specific_event(
                        inner_event,
                        &mut wait_to_complete_batching,
                    )
                });
                if let true = wait_to_complete_batching {
                    Poll::Pending
                } else {
                    Poll::Ready(event)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
