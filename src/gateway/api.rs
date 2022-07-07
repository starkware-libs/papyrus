use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};

use crate::starknet::{BlockHash, BlockNumber, ContractAddress, StarkFelt, StorageKey};

use super::objects::Block;

#[derive(Debug, Copy, Clone, Deserialize, PartialEq, Serialize)]
pub enum BlockResponseScope {
    #[serde(rename = "TXN_HASH")]
    TransactionHashes,
    #[serde(rename = "FULL_TXNS")]
    FullTransactions,
    #[serde(rename = "FULL_TXN_AND_RECEIPTS")]
    FullTransactionsAndReceipts,
}

impl Default for BlockResponseScope {
    fn default() -> Self {
        BlockResponseScope::TransactionHashes
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Tag {
    /// The most recent fully constructed block
    #[serde(rename = "latest")]
    Latest,
    /// Currently constructed block
    #[serde(rename = "pending")]
    Pending,
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum BlockNumberOrTag {
    Number(BlockNumber),
    Tag(Tag),
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum BlockHashOrTag {
    Hash(BlockHash),
    Tag(Tag),
}

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum JsonRpcError {
    #[error("There are no blocks.")]
    NoBlocks,
    #[error("Contract not found.")]
    ContractNotFound = 20,
    #[error("Invalid block hash.")]
    InvalidBlockHash = 24,
    #[error("Invalid block number.")]
    InvalidBlockNumber = 26,
}

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    fn block_number(&self) -> Result<BlockNumber, Error>;

    /// Gets block information given the block number (its height).
    #[method(name = "getBlockByNumber")]
    fn get_block_by_number(
        &self,
        block_number: BlockNumberOrTag,
        requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error>;

    /// Gets block information given the block id.
    #[method(name = "getBlockByHash")]
    fn get_block_by_hash(
        &self,
        block_hash: BlockHashOrTag,
        requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error>;

    /// Gets the value of the storage at the given address, key, and block.
    #[method(name = "getStorageAt")]
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_hash: BlockHashOrTag,
    ) -> Result<StarkFelt, Error>;
}
