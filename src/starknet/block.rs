use serde::{Deserialize, Serialize};

pub struct BlockHeader {}
pub struct BlockBody {}
pub struct BlockHash([u64; 4]);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockNumber(pub u64);
