use serde::{Deserialize, Serialize};

use super::StarkHash;

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);
