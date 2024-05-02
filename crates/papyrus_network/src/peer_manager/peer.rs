// using chrono time and not std since std does not have the ability for std::time::Instance to
// represent the maximum time of the system.
use chrono::{DateTime, Duration, Utc};
use libp2p::swarm::ConnectionId;
use libp2p::{Multiaddr, PeerId};
#[cfg(test)]
use mockall::automock;
use tracing::debug;

use super::ReputationModifier;

#[cfg_attr(test, automock)]
pub trait PeerTrait {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self;

    fn update_reputation(&mut self, reason: ReputationModifier);

    fn peer_id(&self) -> PeerId;

    fn multiaddr(&self) -> Multiaddr;

    fn set_timeout_duration(&mut self, duration: Duration);

    fn is_blocked(&self) -> bool;

    fn connection_ids(&self) -> &Vec<ConnectionId>;

    fn add_connection_id(&mut self, connection_id: ConnectionId);

    fn remove_connection_id(&mut self, connection_id: ConnectionId);
}

#[derive(Clone)]
pub struct Peer {
    peer_id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Option<DateTime<Utc>>,
    timeout_duration: Option<Duration>,
    connection_ids: Vec<ConnectionId>,
}

impl PeerTrait for Peer {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
        Self {
            peer_id,
            multiaddr,
            timeout_duration: None,
            timed_out_until: None,
            connection_ids: Vec::new(),
        }
    }

    fn update_reputation(&mut self, _reason: ReputationModifier) {
        if let Some(timeout_duration) = self.timeout_duration {
            self.timed_out_until =
                Utc::now().checked_add_signed(timeout_duration).or(Some(DateTime::<Utc>::MAX_UTC));
            return;
        }
        debug!("Timeout duration not set for peer: {:?}", self.peer_id);
    }

    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    fn multiaddr(&self) -> Multiaddr {
        self.multiaddr.clone()
    }

    fn set_timeout_duration(&mut self, duration: Duration) {
        self.timeout_duration = Some(duration);
    }

    fn is_blocked(&self) -> bool {
        if let Some(timed_out_until) = self.timed_out_until {
            timed_out_until > Utc::now()
        } else {
            false
        }
    }

    fn connection_ids(&self) -> &Vec<ConnectionId> {
        &self.connection_ids
    }

    fn add_connection_id(&mut self, connection_id: ConnectionId) {
        self.connection_ids.push(connection_id);
    }

    fn remove_connection_id(&mut self, connection_id: ConnectionId) {
        self.connection_ids.retain(|&id| id != connection_id);
    }
}
