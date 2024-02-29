use futures::stream::Stream;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};

use crate::block_headers::behaviour::{
    Behaviour as BlockHeadersBehaviour,
    PeerNotConnected,
    SessionIdNotFoundError,
};
use crate::db_executor::Data;
use crate::streamed_data::{InboundSessionId, OutboundSessionId};
use crate::{InternalQuery, PeerAddressConfig};
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

    fn dial(&mut self, peer: PeerAddressConfig) -> Result<(), DialError>;
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

    fn dial(&mut self, peer: PeerAddressConfig) -> Result<(), DialError> {
        let address = format!("/ip4/{}", peer.ip)
            .parse::<Multiaddr>()
            .expect("string to multiaddr failed")
            .with(Protocol::Tcp(peer.tcp_port));

        self.dial(DialOpts::peer_id(peer.peer_id).addresses(vec![address]).build())
    }
}
