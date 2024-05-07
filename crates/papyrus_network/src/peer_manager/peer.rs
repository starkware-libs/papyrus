use std::time::{Duration, Instant};

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

    /// Returns Instant::now if not blocked.
    fn blocked_until(&self) -> Instant;

    // TODO: add support for multiple connections for a peer
    fn connection_id(&self) -> Option<ConnectionId>;

    fn set_connection_id(&mut self, connection_id: Option<ConnectionId>);
}

#[derive(Clone)]
pub struct Peer {
    peer_id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Option<Instant>,
    timeout_duration: Option<Duration>,
    connection_id: Option<ConnectionId>,
}

impl PeerTrait for Peer {
    fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
        Self {
            peer_id,
            multiaddr,
            timeout_duration: None,
            timed_out_until: None,
            connection_id: None,
        }
    }

    fn update_reputation(&mut self, _reason: ReputationModifier) {
        if let Some(timeout_duration) = self.timeout_duration {
            self.timed_out_until = Some(Instant::now() + timeout_duration);
        } else {
            debug!("Timeout duration not set for peer: {:?}", self.peer_id);
        }
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
            timed_out_until > Instant::now()
        } else {
            false
        }
    }

    fn blocked_until(&self) -> Instant {
        self.timed_out_until.unwrap_or_else(|| Instant::now())
    }

    fn connection_id(&self) -> Option<ConnectionId> {
        self.connection_id
    }

    fn set_connection_id(&mut self, connection_id: Option<ConnectionId>) {
        self.connection_id = connection_id;
    }
}
