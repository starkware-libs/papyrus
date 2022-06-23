use serde::{Deserialize, Serialize};

use super::{StarkFelt, StarkHash};

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(pub StarkHash);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct Nonce(pub StarkFelt);
