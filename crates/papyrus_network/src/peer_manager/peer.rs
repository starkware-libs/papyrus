use chrono::{DateTime, Duration, Utc};
use libp2p::{Multiaddr, PeerId};
#[cfg(test)]
use mockall::automock;

use super::ReputationModifier;

#[cfg_attr(test, automock)]
pub trait PeerTrait {
    fn update_reputation(&mut self, reason: ReputationModifier) -> Result<(), PeerError>;

    fn peer_id(&self) -> PeerId;

    fn multiaddr(&self) -> Multiaddr;

    fn set_timeout_duration(&mut self, duration: Duration);

    fn is_blocked(&self) -> bool;
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum PeerError {
    #[error("Timeout duration not set for peer")]
    TimeoutDurationNotSet,
}

#[derive(Clone)]
pub(crate) struct Peer {
    peer_id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Option<DateTime<Utc>>,
    timeout_duration: Option<Duration>,
}

#[allow(dead_code)]
impl Peer {
    pub fn new(peer_id: PeerId, multiaddr: Multiaddr) -> Self {
        Self { peer_id, multiaddr, timeout_duration: None, timed_out_until: None }
    }
}

impl PeerTrait for Peer {
    fn update_reputation(&mut self, _reason: ReputationModifier) -> Result<(), PeerError> {
        if let Some(timeout_duration) = self.timeout_duration {
            self.timed_out_until = Utc::now()
                .checked_add_signed(timeout_duration)
                .or_else(|| Some(DateTime::<Utc>::MAX_UTC));
            return Ok(());
        }
        Err(PeerError::TimeoutDurationNotSet)
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
}
