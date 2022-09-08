use serde::{Deserialize, Serialize};

use crate::serde_utils::DeserializationError;
use crate::state::PatriciaKey;
use crate::{StarkFelt, StarkHash};

/// The address of a StarkNet contract.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(PatriciaKey);

impl TryFrom<StarkHash> for ContractAddress {
    type Error = DeserializationError;
    fn try_from(hash: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::new(hash)?))
    }
}

/// The hash of a StarkNet [ContractClass](`super::ContractClass`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(StarkHash);

impl ClassHash {
    pub fn new(hash: StarkHash) -> Self {
        Self(hash)
    }
}

/// The nonce of a StarkNet contract.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Nonce(StarkFelt);

impl Nonce {
    pub fn new(felt: StarkFelt) -> Self {
        Self(felt)
    }
}

impl Default for Nonce {
    fn default() -> Self {
        Nonce(StarkFelt::from_u64(0))
    }
}
