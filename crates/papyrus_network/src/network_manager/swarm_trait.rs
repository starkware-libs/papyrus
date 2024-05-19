use futures::stream::Stream;
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm};
use tracing::error;

use crate::gossipsub_impl::Topic;
use crate::streamed_bytes::behaviour::{PeerNotConnected, SessionIdNotFoundError};
use crate::streamed_bytes::{Bytes, InboundSessionId, OutboundSessionId};
use crate::{mixed_behaviour, Protocol};

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

    fn dial(&mut self, peer_multiaddr: Multiaddr) -> Result<(), DialError>;

    fn num_connected_peers(&self) -> usize;

    fn close_inbound_session(
        &mut self,
        session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour;

    fn add_external_address(&mut self, address: Multiaddr);

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError>;

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash);
}

impl SwarmTrait for Swarm<mixed_behaviour::MixedBehaviour> {
    fn send_length_prefixed_data(
        &mut self,
        data: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().streamed_bytes.send_length_prefixed_data(data, inbound_session_id)
    }

    // TODO: change this function signature
    fn send_query(
        &mut self,
        query: Vec<u8>,
        _peer_id: PeerId,
        protocol: Protocol,
    ) -> Result<OutboundSessionId, PeerNotConnected> {
        Ok(self.behaviour_mut().streamed_bytes.start_query(query, protocol.into()))
    }

    fn dial(&mut self, peer_multiaddr: Multiaddr) -> Result<(), DialError> {
        self.dial(DialOpts::from(peer_multiaddr))
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

    fn add_external_address(&mut self, address: Multiaddr) {
        self.add_external_address(address);
    }

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError> {
        self.behaviour_mut().gossipsub.subscribe(topic).map(|_| ())
    }

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash) {
        let result = self.behaviour_mut().gossipsub.publish(topic_hash.clone(), message);
        if let Err(err) = result {
            // TODO(shahak): Consider reporting to the subscriber broadcast failures or retrying
            // upon failure.
            error!(
                "Error occured while broadcasting a message to the topic with hash \
                 {topic_hash:?}: {err:?}"
            );
        }
    }
}
