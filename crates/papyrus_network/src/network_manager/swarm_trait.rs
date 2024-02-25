use std::str::FromStr;

use futures::stream::Stream;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};

use crate::block_headers::behaviour::{
    Behaviour as BlockHeadersBehaviour, PeerNotConnected, SessionIdNotFoundError,
};
use crate::db_executor::Data;
use crate::streamed_data::{InboundSessionId, OutboundSessionId};
use crate::InternalQuery;
pub type Event = SwarmEvent<<BlockHeadersBehaviour as NetworkBehaviour>::ToSwarm>;

pub trait SwarmTrait: Stream<Item = Event> + Unpin {
    fn send_data(
        &mut self,
        data: Data,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn send_query(
        &mut self,
        query: InternalQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected>;

    fn dial(&mut self, peer_id: PeerId) -> Result<(), DialError>;
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
        query: InternalQuery,
        peer_id: PeerId,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.behaviour_mut().send_query(query, peer_id)
    }

    fn dial(&mut self, peer_id: PeerId) -> Result<(), DialError> {
        let addresses =
            vec![Multiaddr::from_str("/ip4/127.0.0.1/tcp/10000")
                .expect("string to multiaddr failed")];
        self.dial(DialOpts::peer_id(peer_id).addresses(addresses).build())
    }
}
