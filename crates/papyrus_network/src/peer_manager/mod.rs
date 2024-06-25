use std::collections::HashMap;

use chrono::Duration;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::ToSwarm;
use libp2p::PeerId;
use tracing::info;

pub use self::behaviour_impl::ToOtherBehaviourEvent;
use self::peer::PeerTrait;
use crate::discovery::identify_impl::IdentifyToOtherBehaviourEvent;
use crate::mixed_behaviour::BridgedBehaviour;
use crate::sqmr::OutboundSessionId;
use crate::{discovery, mixed_behaviour, sqmr};

pub(crate) mod behaviour_impl;
pub(crate) mod peer;
#[cfg(test)]
mod test;

#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum ReputationModifier {
    // TODO: Implement this enum
    Bad,
}

pub struct PeerManager<P: PeerTrait + 'static> {
    peers: HashMap<PeerId, P>,
    // TODO: consider implementing a cleanup mechanism to not store all queries forever
    session_to_peer_map: HashMap<OutboundSessionId, PeerId>,
    config: PeerManagerConfig,
    last_peer_index: usize,
    pending_events: Vec<ToSwarm<ToOtherBehaviourEvent, libp2p::swarm::THandlerInEvent<Self>>>,
    peers_pending_dial_with_sessions: HashMap<PeerId, Vec<OutboundSessionId>>,
    sessions_received_when_no_peers: Vec<OutboundSessionId>,
}

#[derive(Clone)]
pub struct PeerManagerConfig {
    target_num_for_peers: usize,
    blacklist_timeout: Duration,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum PeerManagerError {
    #[error("No such peer: {0}")]
    NoSuchPeer(PeerId),
    #[error("No such session: {0}")]
    NoSuchSession(OutboundSessionId),
    #[error("Peer is blocked: {0}")]
    PeerIsBlocked(PeerId),
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self { target_num_for_peers: 100, blacklist_timeout: Duration::max_value() }
    }
}

#[allow(dead_code)]
impl<P> PeerManager<P>
where
    P: PeerTrait,
{
    pub(crate) fn new(config: PeerManagerConfig) -> Self {
        let peers = HashMap::new();
        Self {
            peers,
            session_to_peer_map: HashMap::new(),
            config,
            last_peer_index: 0,
            pending_events: Vec::new(),
            peers_pending_dial_with_sessions: HashMap::new(),
            sessions_received_when_no_peers: Vec::new(),
        }
    }

    fn add_peer(&mut self, mut peer: P) {
        info!("Peer Manager found new peer {:?}", peer.peer_id());
        peer.set_timeout_duration(self.config.blacklist_timeout);
        self.peers.insert(peer.peer_id(), peer);
        for outbound_session_id in std::mem::take(&mut self.sessions_received_when_no_peers) {
            self.assign_peer_to_session(outbound_session_id);
        }
    }

    #[cfg(test)]
    fn get_mut_peer(&mut self, peer_id: PeerId) -> Option<&mut P> {
        self.peers.get_mut(&peer_id)
    }

    // TODO(shahak): Remove return value and use events in tests.
    fn assign_peer_to_session(&mut self, outbound_session_id: OutboundSessionId) -> Option<PeerId> {
        // TODO: consider moving this logic to be async (on a different tokio task)
        // until then we can return the assignment even if we use events for the notification.
        if self.peers.is_empty() {
            self.sessions_received_when_no_peers.push(outbound_session_id);
            return None;
        }
        let peer = self
            .peers
            .iter()
            .skip(self.last_peer_index)
            .find(|(_, peer)| !peer.is_blocked())
            .or_else(|| {
                self.peers.iter().take(self.last_peer_index).find(|(_, peer)| !peer.is_blocked())
            });
        self.last_peer_index = (self.last_peer_index + 1) % self.peers.len();
        peer.map(|(peer_id, peer)| {
            // TODO: consider not allowing reassignment of the same session
            self.session_to_peer_map.insert(outbound_session_id, *peer_id);
            let peer_connection_ids = peer.connection_ids();
            if !peer_connection_ids.is_empty() {
                let connection_id = peer_connection_ids[0];
                info!(
                    "Session {:?} assigned to peer {:?} with connection id: {:?}",
                    outbound_session_id, peer_id, connection_id
                );
                self.pending_events.push(ToSwarm::GenerateEvent(
                    ToOtherBehaviourEvent::SessionAssigned {
                        outbound_session_id,
                        peer_id: *peer_id,
                        connection_id,
                    },
                ));
            } else {
                // In case we have a race condition where the connection is closed after we added to
                // the pending list, the reciever will get an error and will need to ask for
                // re-assignment
                if let Some(sessions) = self.peers_pending_dial_with_sessions.get_mut(peer_id) {
                    sessions.push(outbound_session_id);
                } else {
                    self.peers_pending_dial_with_sessions
                        .insert(*peer_id, vec![outbound_session_id]);
                }
                info!("Dialing peer {:?} with multiaddr {:?}", peer_id, peer.multiaddr());
                self.pending_events.push(ToSwarm::Dial {
                    opts: DialOpts::peer_id(*peer_id).addresses(vec![peer.multiaddr()]).build(),
                });
            }
            *peer_id
        })
    }

    pub(crate) fn report_peer(
        &mut self,
        peer_id: PeerId,
        reason: ReputationModifier,
    ) -> Result<(), PeerManagerError> {
        // TODO(shahak): Add time blacklisted to log.
        info!("Peer {:?} reported as misbehaving.", peer_id);
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            peer.update_reputation(reason);
            Ok(())
        } else {
            Err(PeerManagerError::NoSuchPeer(peer_id))
        }
    }

    fn report_session(
        &mut self,
        outbound_session_id: OutboundSessionId,
        reason: ReputationModifier,
    ) -> Result<(), PeerManagerError> {
        if let Some(peer_id) = self.session_to_peer_map.get(&outbound_session_id) {
            if let Some(peer) = self.peers.get_mut(peer_id) {
                peer.update_reputation(reason);
                Ok(())
            } else {
                Err(PeerManagerError::NoSuchPeer(*peer_id))
            }
        } else {
            Err(PeerManagerError::NoSuchSession(outbound_session_id))
        }
    }

    fn more_peers_needed(&self) -> bool {
        // TODO: consider if we should count blocked peers (and in what cases? what if they are
        // blocked temporarily?)
        self.peers.len() < self.config.target_num_for_peers
    }
}

impl From<ToOtherBehaviourEvent> for mixed_behaviour::Event {
    fn from(event: ToOtherBehaviourEvent) -> Self {
        Self::ToOtherBehaviourEvent(mixed_behaviour::ToOtherBehaviourEvent::PeerManager(event))
    }
}

impl<P: PeerTrait + 'static> BridgedBehaviour for PeerManager<P> {
    fn on_other_behaviour_event(&mut self, event: &mixed_behaviour::ToOtherBehaviourEvent) {
        match event {
            mixed_behaviour::ToOtherBehaviourEvent::Sqmr(
                sqmr::ToOtherBehaviourEvent::RequestPeerAssignment { outbound_session_id },
            ) => {
                self.assign_peer_to_session(*outbound_session_id);
            }
            mixed_behaviour::ToOtherBehaviourEvent::Identify(
                IdentifyToOtherBehaviourEvent::FoundListenAddresses { peer_id, listen_addresses },
            )
            | mixed_behaviour::ToOtherBehaviourEvent::Discovery(
                discovery::ToOtherBehaviourEvent::FoundListenAddresses {
                    peer_id,
                    listen_addresses,
                },
            ) => {
                // TODO(shahak): Handle changed addresses
                if self.peers.contains_key(peer_id) {
                    return;
                }
                // TODO(shahak): Track multiple addresses per peer.
                let Some(address) = listen_addresses.first() else {
                    return;
                };

                let peer = P::new(*peer_id, address.clone());
                self.add_peer(peer);
                if !self.more_peers_needed() {
                    // TODO: consider how and in which cases we resume discovery
                    self.pending_events.push(libp2p::swarm::ToSwarm::GenerateEvent(
                        ToOtherBehaviourEvent::PauseDiscovery,
                    ))
                }
            }
            _ => {}
        }
    }
}
