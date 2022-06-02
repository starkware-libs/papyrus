use serde::{Deserialize, Serialize};

use super::block::HexAsBytes;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "HexAsBytes<32>")]
pub struct StarkHash(pub [u8; 32]);

impl From<HexAsBytes<32_usize>> for StarkHash {
    fn from(v: HexAsBytes<32_usize>) -> Self {
        StarkHash(v.0)
    }
}
