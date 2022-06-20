use serde::{Deserialize, Serialize};
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StarkHash(pub [u8; 32]);
pub type StarkFelt = StarkHash;
