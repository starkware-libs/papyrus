use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};

pub mod block_hash;
pub mod deprecated_class_abi;
pub mod metrics;
pub mod patricia_hash_tree;
pub mod pending_classes;
pub mod state;
pub mod state_diff_commitment;
pub mod transaction_hash;

#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct TransactionOptions {
    /// Transaction that shouldn't be broadcasted to StarkNet. For example, users that want to
    /// test the execution result of a transaction without the risk of it being rebroadcasted (the
    /// signature will be different while the execution remain the same). Using this flag will
    /// modify the transaction version by setting the 128-th bit to 1.
    pub only_query: bool,
}
