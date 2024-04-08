use std::time::{self, Duration};

use libp2p::{Multiaddr, PeerId};
#[cfg(test)]
use mockall::automock;

use super::ReputationModifier;

#[cfg_attr(test, automock)]
pub trait PeerTrait {
    fn update_reputation(&mut self, reason: ReputationModifier) -> Result<(), PeerError>;

    fn get_id(&self) -> PeerId;

    fn get_multiaddr(&self) -> Multiaddr;

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
    id: PeerId,
    multiaddr: Multiaddr,
    timed_out_until: Option<time::Instant>,
    timeout_duration: Option<Duration>,
}

#[allow(dead_code)]
impl Peer {
    pub fn new(id: PeerId, multiaddr: Multiaddr) -> Self {
        Self { id, multiaddr, timeout_duration: None, timed_out_until: None }
    }
}

impl PeerTrait for Peer {
    fn update_reputation(&mut self, _reason: ReputationModifier) -> Result<(), PeerError> {
        if let Some(timeout_duration) = self.timeout_duration {
            self.timed_out_until = Some(time::Instant::now() + timeout_duration);
            return Ok(());
        }
        Err(PeerError::TimeoutDurationNotSet)
    }

    fn get_id(&self) -> PeerId {
        self.id
    }

    fn get_multiaddr(&self) -> Multiaddr {
        self.multiaddr.clone()
    }

    fn set_timeout_duration(&mut self, duration: Duration) {
        self.timeout_duration = Some(duration);
    }

    fn is_blocked(&self) -> bool {
        if let Some(timed_out_until) = self.timed_out_until {
            timed_out_until > time::Instant::now()
        } else {
            false
        }
    }
}
