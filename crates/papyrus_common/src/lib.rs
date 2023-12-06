use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};

pub mod block_hash;
pub mod metrics;
pub mod patricia_hash_tree;
pub mod pending_classes;
pub mod state;
pub mod transaction_hash;

#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct TransactionOptions {
    /// Transaction that shouldn't be broadcasted to StarkNet. For example, users that want to
    /// test the execution of a transaction without revealing the signature.
    /// Using this flag will modify the transaction version by setting the 128th bit to 1.
    pub only_query: bool,
}
