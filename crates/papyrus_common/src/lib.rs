use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};

pub mod metrics;

#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}
