use serde::{Deserialize, Serialize};

use super::{StarkFelt, StarkHash};
use crate::{shash, StarknetApiError};

/// The address of a StarkNet contract.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, PartialOrd, Ord)]
#[serde(from = "ContractAddress")]
// Invariant: contract addresses are in [1, 2**251 - 256).
pub struct ContractAddress(StarkHash);
impl Default for ContractAddress {
    fn default() -> Self {
        ContractAddress(shash!("0x1"))
    }
}

impl ContractAddress {
    // (2**251 - 256).
    pub const UPPER_BOUND: &'static str =
        "0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00";

    // TODO(yair): check if 0 is valid value for contract addresses.
    pub const LOWER_BOUND: &'static str = "0x0";
    pub fn new(hash: StarkHash) -> Result<Self, StarknetApiError> {
        if hash > shash!(Self::LOWER_BOUND) && hash < shash!(Self::UPPER_BOUND) {
            Ok(Self(hash))
        } else {
            let error_msg = format!(
                "Failed to create contract address. Expected StarkHash in range ({:#?}, {:#?}), \
                 received {:#?}.",
                Self::LOWER_BOUND,
                Self::UPPER_BOUND,
                hash
            );
            log::error!("{}", error_msg);
            Err(StarknetApiError::OutOfRange)
        }
    }
}

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
