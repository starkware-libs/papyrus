use std::task::Poll;

use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    dummy,
    ConnectionClosed,
    ConnectionId,
    DialError,
    DialFailure,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tracing::{debug, error};

use super::peer::PeerTrait;
use super::{PeerManager, PeerManagerError};
use crate::sqmr::OutboundSessionId;

#[derive(Debug)]
pub enum ToOtherBehaviourEvent {
    SessionAssigned {
        outbound_session_id: OutboundSessionId,
        peer_id: PeerId,
        connection_id: ConnectionId,
    },
    PauseDiscovery,
    ResumeDiscovery,
}

impl<P: 'static> NetworkBehaviour for PeerManager<P>
where
    P: PeerTrait,
{
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = ToOtherBehaviourEvent;

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
                error,
                connection_id: _,
            }) => {
                debug!("Dial failure for peer: {}, error: {}", peer_id, error);
                if let DialError::DialPeerConditionFalse(_) = error {
                    debug!(
                        "There is another active connection or a dial attempt in progress, not \
                         doing anything"
                    );
                    return;
                }
                let res = self.report_peer(peer_id, super::ReputationModifier::Bad);
                if res.is_err() {
                    error!("Dial failure of an unknow peer. peer id: {}", peer_id)
                }
                // Re-assign a peer to the session so that a SessionAssgined Event will be emitted.
                // TODO: test this case
                let queries_to_assign =
                    self.session_to_peer_map
                        .iter()
                        .filter_map(|(outbound_session_id, p_id)| {
                            if *p_id == peer_id { Some(*outbound_session_id) } else { None }
                        })
                        .collect::<Vec<_>>();
                for outbound_session_id in queries_to_assign {
                    self.assign_peer_to_session(outbound_session_id);
                }
            }
            libp2p::swarm::FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                if let Some(sessions) = self.peers_pending_dial_with_sessions.remove(&peer_id) {
                    self.pending_events.extend(sessions.iter().map(|outbound_session_id| {
                        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
                            outbound_session_id: *outbound_session_id,
                            peer_id,
                            connection_id,
                        })
                    }));
                    self.peers
                        .get_mut(&peer_id)
                        .expect(
                            "in case we are waiting for a connection established event we assum \
                             the peer is known to the peer manager",
                        )
                        .add_connection_id(connection_id);
                } else {
                    let Some(peer) = self.peers.get_mut(&peer_id) else {
                        // TODO(shahak): Consider tracking connection ids for peers we don't know
                        // yet because once a connection is established we'll shortly receive an
                        // identify message and add this peer.
                        return;
                    };
                    peer.add_connection_id(connection_id);
                }
            }
            libp2p::swarm::FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    let known_connection_ids = peer.connection_ids();
                    if known_connection_ids.contains(&connection_id) {
                        peer.remove_connection_id(connection_id);
                    } else {
                        error!(
                            "Connection closed event for a peer with a different connection id. \
                             known connection ids: {:?}, emitted connection id: {}",
                            known_connection_ids, connection_id
                        );
                    }
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
