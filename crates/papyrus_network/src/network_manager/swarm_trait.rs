use futures::stream::Stream;
use libp2p::multiaddr::Protocol as LibP2pProtocol;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};

use crate::streamed_bytes::behaviour::{Behaviour, PeerNotConnected, SessionIdNotFoundError};
use crate::streamed_bytes::{InboundSessionId, OutboundSessionId};
use crate::{PeerAddressConfig, Protocol};

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

    fn dial(&mut self, peer: PeerAddressConfig) -> Result<(), DialError>;
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

    fn dial(&mut self, peer: PeerAddressConfig) -> Result<(), DialError> {
        let address = format!("/ip4/{}", peer.ip)
            .parse::<Multiaddr>()
            .expect("string to multiaddr failed")
            .with(LibP2pProtocol::Tcp(peer.tcp_port));

        self.dial(DialOpts::peer_id(peer.peer_id).addresses(vec![address]).build())
    }
}
