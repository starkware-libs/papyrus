use futures::stream::Stream;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{PeerId, Swarm};

// use crate::block_headers::behaviour::{
//     Behaviour as BlockHeadersBehaviour, PeerNotConnected, SessionIdNotFoundError,
// };
use crate::streamed_bytes::behaviour::{Behaviour, PeerNotConnected, SessionIdNotFoundError};
use crate::streamed_bytes::{InboundSessionId, OutboundSessionId};
use crate::Protocol;

pub type Event = SwarmEvent<<Behaviour as NetworkBehaviour>::ToSwarm>;

pub trait SwarmTrait: Stream<Item = Event> + Unpin {
    fn send_data(
        &mut self,
        data: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn send_query(
        &mut self,
        query: Vec<u8>,
        peer_id: PeerId,
        protocol: Protocol,
    ) -> Result<OutboundSessionId, PeerNotConnected>;
}

impl SwarmTrait for Swarm<Behaviour> {
    fn send_data(
        &mut self,
        data: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().send_data(data, inbound_session_id)
    }

    fn send_query(
        &mut self,
        query: Vec<u8>,
        peer_id: PeerId,
        protocol: Protocol,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.behaviour_mut().send_query(query, peer_id, protocol.into())
    }
}
