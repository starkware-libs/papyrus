use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};

use crate::starknet::{BlockHash, BlockNumber};

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
#[serde(untagged)]
pub enum BlockNumberOrTag {
    Number(BlockNumber),
    Tag(Tag),
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BlockHashOrTag {
    Hash(BlockHash),
    Tag(Tag),
}

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum JsonRpcError {
    #[error("There are no blocks.")]
    NoBlocks,
    #[error("Invalid block number.")]
    InvalidBlockNumber = 26,
    #[error("Invalid block hash.")]
    InvalidBlockHash = 24,
}

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    async fn block_number(&self) -> Result<BlockNumber, Error>;

    /// Gets block information given the block number (its height).
    #[method(name = "getBlockByNumber")]
    async fn get_block_by_number(
        &self,
        block_number: BlockNumberOrTag,
        requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error>;

    /// Gets block information given the block id.
    #[method(name = "getBlockByHash")]
    async fn get_block_by_hash(
        &self,
        block_hash: BlockHashOrTag,
        requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error>;
}
