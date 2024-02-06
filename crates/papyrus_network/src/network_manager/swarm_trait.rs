use futures::stream::Stream;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{PeerId, Swarm};

use crate::block_headers::behaviour::{
    Behaviour as BlockHeadersBehaviour,
    PeerNotConnected,
    SessionIdNotFoundError,
};
use crate::db_executor::Data;
use crate::streamed_data::{InboundSessionId, OutboundSessionId};
use crate::BlockQuery;

pub type Event = SwarmEvent<<BlockHeadersBehaviour as NetworkBehaviour>::ToSwarm>;

pub trait SwarmTrait: Stream<Item = Event> + Unpin {
    fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn send_query(
        &mut self,
        query: BlockQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected>;
}

impl SwarmTrait for Swarm<BlockHeadersBehaviour> {
    fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().send_data(data, inbound_session_id)
    }

    fn send_query(
        &mut self,
        query: BlockQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.behaviour_mut().send_query(query, peer_id)
    }
}
