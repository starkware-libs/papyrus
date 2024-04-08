use std::collections::HashMap;
use std::time::Duration;

use libp2p::{Multiaddr, PeerId};

use self::peer::{PeerError, PeerTrait};
use crate::db_executor::QueryId;

mod peer;
#[cfg(test)]
mod test;

#[cfg_attr(test, derive(Debug, PartialEq))]
#[allow(dead_code)]
pub enum ReputationModifier {
    // TODO: Implement this enum
    Bad,
}

struct PeerManager<P> {
    peers: HashMap<PeerId, P>,
    query_to_peer_map: HashMap<QueryId, PeerId>,
    config: PeerManagerConfig,
    last_peer_idx: usize,
}

#[derive(Clone)]
struct PeerManagerConfig {
    target_num_for_peers: usize,
    blacklist_timeout: Duration,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum PeerManagerError {
    #[error("No such peer: {0}")]
    NoSuchPeer(PeerId),
    #[error(transparent)]
    PeerError(#[from] PeerError),
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self { target_num_for_peers: 100, blacklist_timeout: Duration::MAX }
    }
}

#[allow(dead_code)]
impl<P> PeerManager<P>
where
    P: PeerTrait,
{
    pub fn new(config: PeerManagerConfig) -> Self {
        let peers = HashMap::new();
        Self { peers, query_to_peer_map: HashMap::new(), config, last_peer_idx: 0 }
    }

    fn add_peer(&mut self, mut peer: P) {
        peer.set_timeout_duration(self.config.blacklist_timeout);
        self.peers.insert(peer.get_id(), peer);
    }

    #[cfg(test)]
    fn get_mut_peer(&mut self, peer_id: PeerId) -> Option<&mut P> {
        self.peers.get_mut(&peer_id)
    }

    fn assign_peer_to_query(&mut self, query_id: QueryId) -> Option<(PeerId, Multiaddr)> {
        if self.peers.is_empty() {
            return None;
        }
        let peer = self.peers.iter().skip(self.last_peer_idx).find(|(_, peer)| !peer.is_blocked());
        self.last_peer_idx = (self.last_peer_idx + 1) % self.peers.len();
        peer.map(|(peer_id, peer)| {
            self.query_to_peer_map.insert(query_id, *peer_id);
            (*peer_id, peer.get_multiaddr())
        })
    }

    fn report_peer(
        &mut self,
        peer_id: PeerId,
        reason: ReputationModifier,
    ) -> Result<(), PeerManagerError> {
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            peer.update_reputation(reason)?;
            Ok(())
        } else {
            Err(PeerManagerError::NoSuchPeer(peer_id))
        }
    }

    fn report_query(
        &mut self,
        query_id: QueryId,
        reason: ReputationModifier,
    ) -> Result<(), PeerManagerError> {
        if let Some(peer_id) = self.query_to_peer_map.get(&query_id) {
            if let Some(peer) = self.peers.get_mut(peer_id) {
                peer.update_reputation(reason)?;
                Ok(())
            } else {
                Err(PeerManagerError::NoSuchPeer(*peer_id))
            }
        } else {
            Err(PeerManagerError::NoSuchPeer(PeerId::random()))
        }
    }

    fn more_peers_needed(&self) -> bool {
        self.peers.len() < self.config.target_num_for_peers
    }
}
