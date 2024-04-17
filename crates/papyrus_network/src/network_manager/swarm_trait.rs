use futures::stream::Stream;
use libp2p::multiaddr::Protocol as LibP2pProtocol;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};

use crate::main_behaviour::mixed_behaviour;
use crate::streamed_bytes::behaviour::{PeerNotConnected, SessionIdNotFoundError};
use crate::streamed_bytes::{InboundSessionId, OutboundSessionId};
use crate::{PeerAddressConfig, Protocol};

pub type Event = SwarmEvent<<mixed_behaviour::MixedBehaviour as NetworkBehaviour>::ToSwarm>;

pub trait SwarmTrait: Stream<Item = Event> + Unpin {
    fn send_length_prefixed_data(
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

    fn num_connected_peers(&self) -> usize;

    fn close_inbound_session(
        &mut self,
        session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour;
}

impl SwarmTrait for Swarm<mixed_behaviour::MixedBehaviour> {
    fn send_length_prefixed_data(
        &mut self,
        data: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().streamed_bytes.send_length_prefixed_data(data, inbound_session_id)
    }

    fn send_query(
        &mut self,
        query: Vec<u8>,
        peer_id: PeerId,
        protocol: Protocol,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        self.behaviour_mut().streamed_bytes.send_query(query, peer_id, protocol.into())
    }

    fn dial(&mut self, peer: PeerAddressConfig) -> Result<(), DialError> {
        let address = format!("/ip4/{}", peer.ip)
            .parse::<Multiaddr>()
            .expect("string to multiaddr failed")
            .with(LibP2pProtocol::Tcp(peer.tcp_port));

        self.dial(DialOpts::peer_id(peer.peer_id).addresses(vec![address]).build())
    }

    fn num_connected_peers(&self) -> usize {
        self.network_info().num_peers()
    }
    fn close_inbound_session(
        &mut self,
        session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().streamed_bytes.close_inbound_session(session_id)
    }

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour {
        self.behaviour_mut()
    }
}
