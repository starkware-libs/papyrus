use std::task::Poll;

use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{dummy, ConnectionClosed, DialFailure, NetworkBehaviour};
use libp2p::Multiaddr;
use tracing::error;

use super::peer::PeerTrait;
use super::{PeerManager, PeerManagerError};
use crate::streamed_bytes;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Event {
    NotifyStreamedBytes(streamed_bytes::behaviour::FromOtherBehaviour),
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

    // TODO: in case we want to deny a connection based on the remote address
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
        Ok(dummy::ConnectionHandler)
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: libp2p::PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        _event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        // no events from dummy handler
    }

    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<'_>) {
        // TODO: consider if we should handle other events
        #[allow(clippy::single_match)]
        match event {
            libp2p::swarm::FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id),
                error: _,
                connection_id: _,
            }) => {
                let res = self.report_peer(peer_id, super::ReputationModifier::Bad);
                if res.is_err() {
                    error!("Dial failure of an unknow peer. peer id: {}", peer_id)
                }
                // Re-assign a peer to the query so that a QueryAssgined Event will be emitted.
                // TODO: test this case
                let queries_to_assign = self
                    .query_to_peer_map
                    .iter()
                    .filter_map(
                        |(query_id, p_id)| {
                            if *p_id == peer_id { Some(*query_id) } else { None }
                        },
                    )
                    .collect::<Vec<_>>();
                for query_id in queries_to_assign {
                    self.assign_peer_to_query(query_id);
                }
            }
            libp2p::swarm::FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                if let Some(events) = self.peer_pending_dial_with_events.remove(&peer_id) {
                    self.pending_events.extend(events);
                    self.peers
                        .get_mut(&peer_id)
                        .expect(
                            "in case we are waiting for a connection established event we assum \
                             the peer is known to the peer manager",
                        )
                        .set_connection_id(Some(connection_id));
                };
            }
            libp2p::swarm::FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    if let Some(known_connection_id) = peer.connection_id() {
                        if known_connection_id == connection_id {
                            peer.set_connection_id(None);
                        } else {
                            error!(
                                "Connection closed event for a peer with a different connection \
                                 id. known connection id: {}, emitted connection id: {}",
                                known_connection_id, connection_id
                            );
                            return;
                        }
                    } else {
                        error!("Connection closed event for a peer without a connection id");
                        return;
                    }
                    peer.set_connection_id(None);
                }
            }
            _ => {}
        }
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        self.pending_events.pop().map(Poll::Ready).unwrap_or(Poll::Pending)
    }
}
