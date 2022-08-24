use serde::{Deserialize, Serialize};

use super::{StarkFelt, StarkHash};

/// The address of a StarkNet contract.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Serialize, PartialOrd, Ord)]
#[serde(from = "ContractAddress")]
// Invariant: contract addresses are in [1, 2**251 - 256) enforced by manual deserialization.
pub struct ContractAddress(pub StarkHash);

/// The hash of a StarkNet [ContractClass](`super::ContractClass`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(pub StarkHash);

/// The nonce of a StarkNet contract.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Nonce(pub StarkFelt);
impl Default for Nonce {
    fn default() -> Self {
        Nonce(StarkFelt::from_u64(0))
    }
}
