#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use std::fmt::Debug;
use std::io::Error;

use serde::{Deserialize, Serialize};

use super::serde_utils::{
    bytes_from_hex_str, hex_str_from_bytes, HexAsBytes, NonPrefixedHexAsBytes, PrefixedHexAsBytes,
};
use super::StarknetApiError;

/// Genesis state hash.
pub const GENESIS_HASH: &str = "0x0";
// Felt encoding constants.
const CHOOSER_FULL: u8 = 15;
const CHOOSER_HALF: u8 = 14;

// TODO: Move to a different crate.
/// A hash in StarkNet.
#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(try_from = "PrefixedHexAsBytes<32_usize>", into = "PrefixedHexAsBytes<32_usize>")]
pub struct StarkHash([u8; 32]);
impl StarkHash {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}
impl TryFrom<PrefixedHexAsBytes<32_usize>> for StarkHash {
    type Error = StarknetApiError;
    fn try_from(val: PrefixedHexAsBytes<32_usize>) -> Result<Self, Self::Error> {
        StarkHash::new(val.0)
    }
}
// TODO(anatg): Remove once Starknet sequencer returns the global root hash as a hex string with a
// "0x" prefix.
impl TryFrom<NonPrefixedHexAsBytes<32_usize>> for StarkHash {
    type Error = StarknetApiError;
    fn try_from(val: NonPrefixedHexAsBytes<32_usize>) -> Result<Self, Self::Error> {
        StarkHash::new(val.0)
    }
}
impl From<StarkHash> for PrefixedHexAsBytes<32_usize> {
    fn from(val: StarkHash) -> Self {
        HexAsBytes(val.0)
    }
}

impl Debug for StarkHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("0x{}", hex::encode(self.0));
        f.debug_tuple("StarkHash").field(&s).finish()
    }
}

impl StarkHash {
    /// Returns a new [`StarkHash`].
    pub fn new(bytes: [u8; 32]) -> Result<StarkHash, StarknetApiError> {
        // msb nibble must be 0. This is not a tight bound.
        if bytes[0] >= 0x10 {
            return Err(StarknetApiError::OutOfRange {
                string: hex_str_from_bytes::<32, true>(bytes),
            });
        }
        Ok(Self(bytes))
    }
    /// Returns a [`StarkHash`] corresponding to `hex_str`.
    pub fn from_hex(hex_str: &str) -> Result<StarkHash, StarknetApiError> {
        let bytes = bytes_from_hex_str::<32, true>(hex_str)?;
        Self::new(bytes)
    }
    /// Returns a [`StarkHash`] corresponding to `val`.
    pub fn from_u64(val: u64) -> StarkHash {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&val.to_be_bytes());
        StarkHash(bytes)
    }

    /// Storage efficient serialization for field elements.
    pub fn serialize(&self, res: &mut impl std::io::Write) -> Result<(), Error> {
        // We use the fact that bytes[0] < 0x10 and encode the size of the felt in the 4 most
        // significant bits of the serialization, which we call `chooser`. We assume that 128 bit
        // felts are prevalent (because of how uint256 is encoded in felts).

        // The first i for which nibbles 2i+1, 2i+2 are nonzero. Note that the first nibble is
        // always 0.
        let mut first_index = 31;
        for i in 0..32 {
            let value = self.0[i];
            if value == 0 {
                continue;
            } else if value < 16 {
                // Can encode the chooser and the value on a single byte.
                first_index = i;
            } else {
                // The chooser is encoded with the first nibble of the value.
                first_index = i - 1;
            }
            break;
        }
        let chooser = if first_index < 15 {
            // For 34 up to 63 nibble felts: chooser == 15, serialize using 32 bytes.
            first_index = 0;
            CHOOSER_FULL
        } else if first_index < 18 {
            // For 28 up to 33 nibble felts: chooser == 14, serialize using 17 bytes.
            first_index = 15;
            CHOOSER_HALF
        } else {
            // For up to 27 nibble felts: serialize the lower 1 + (chooser * 2) nibbles of the felt
            // using chooser + 1 bytes.
            (31 - first_index) as u8
        };
        res.write_all(&[(chooser << 4) | self.0[first_index]])?;
        res.write_all(&self.0[first_index + 1..])?;
        Ok(())
    }

    /// Storage efficient deserialization for field elements.
    pub fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        let mut res = [0u8; 32];

        bytes.read_exact(&mut res[..1]).ok()?;
        let first = res[0];
        let chooser: u8 = first >> 4;
        let first = first & 0x0f;

        let first_index = if chooser == CHOOSER_FULL {
            0
        } else if chooser == CHOOSER_HALF {
            15
        } else {
            (31 - chooser) as usize
        };
        res[0] = 0;
        res[first_index] = first;
        bytes.read_exact(&mut res[first_index + 1..]).ok()?;
        Some(Self(res))
    }
}

/// The StarkNet elliptic curve field element.
pub type StarkFelt = StarkHash;

/// A utility macro to create a [`StarkHash`] from a hex string representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! shash {
    ($s:expr) => {
        StarkHash::from_hex($s).unwrap()
    };
}
