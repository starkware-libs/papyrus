use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockTimestamp, GasPrice};
use starknet_api::core::{ContractAddress, GlobalRoot};

use super::block::BlockStatus;
use super::transaction::{Transaction, TransactionReceipt};
use crate::reader::StateDiff;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct PendingData {
    pub block: PendingBlock,
    pub state_update: PendingStateUpdate,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct PendingBlock {
    #[serde(default)]
    pub block_hash: Option<BlockHash>,
    pub parent_block_hash: BlockHash,
    pub status: BlockStatus,
    pub gas_price: GasPrice,
    pub transactions: Vec<Transaction>,
    pub timestamp: BlockTimestamp,
    pub sequencer_address: ContractAddress,
    pub transaction_receipts: Vec<TransactionReceipt>,
    pub starknet_version: String,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct PendingStateUpdate {
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}
