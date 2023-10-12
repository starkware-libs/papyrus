use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};

pub mod block_hash;
pub mod metrics;
pub mod patricia_hash_tree;
pub mod pending_classes;
pub mod transaction_hash;

#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}
