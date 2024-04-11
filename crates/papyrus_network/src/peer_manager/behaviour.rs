use std::task::Poll;

use libp2p::swarm::{dummy, NetworkBehaviour, ToSwarm};
use libp2p::Multiaddr;

use super::peer::PeerTrait;
use super::{PeerManager, PeerManagerError};
use crate::streamed_bytes;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Event {
    NotifyStreamedBytes(streamed_bytes::behaviour::InternalEvent),
}

impl<P: 'static> NetworkBehaviour for PeerManager<P>
where
    P: PeerTrait,
{
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        inbound_peer_id: libp2p::PeerId,
        _local_addr: &libp2p::Multiaddr,
        _remote_addr: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        // TODO: consider implementing a better lookup mechanism in case there's a lot of peers this
        // will be slow
        match self
            .peers
            .iter()
            .find(|(peer_id, peer)| (*peer_id == &inbound_peer_id) && peer.is_blocked())
        {
            Some(_) => Err(libp2p::swarm::ConnectionDenied::new(PeerManagerError::PeerIsBlocked(
                inbound_peer_id,
            ))),
            None => Ok(dummy::ConnectionHandler {}),
        }
    }

    // in case we want to deny a connection based on the remote address
    // we probably need to keep a separate list of banned addresses since extracting it from the
    // peers multiaddrs will be slow
    fn handle_pending_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        Ok(())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: libp2p::PeerId,
        _addr: &libp2p::Multiaddr,
        _role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(dummy::ConnectionHandler {})
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: libp2p::PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        _event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        // no events from dummy handler
    }

    fn on_swarm_event(&mut self, _event: libp2p::swarm::FromSwarm<'_>) {
        unimplemented!()
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        self.pending_events
            .pop()
            .map(|event| Poll::Ready(ToSwarm::GenerateEvent(event)))
            .unwrap_or(Poll::Pending)
    }
}
