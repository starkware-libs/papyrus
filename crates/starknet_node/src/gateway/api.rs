use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
pub use starknet_api::{
    BlockHash, BlockNumber, ClassHash, ContractAddress, StarkFelt, StorageKey, Transaction,
    TransactionHash, TransactionOffsetInBlock,
};

pub use super::objects::{Block, StateUpdate};

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
pub enum BlockId {
    Hash(BlockHash),
    Number(BlockNumber),
    Tag(Tag),
}

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum JsonRpcError {
    #[error("There are no blocks.")]
    NoBlocks,
    #[error("Contract not found.")]
    ContractNotFound = 20,
    #[error("Invalid block id.")]
    InvalidBlockId = 24,
    #[error("Invalid transaction hash.")]
    InvalidTransactionHash = 25,
    #[error("Invalid transaction index in a block.")]
    InvalidTransactionIndex = 27,
}

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    fn block_number(&self) -> Result<BlockNumber, Error>;

    /// Gets block information with transaction hashes given a block identifier.
    #[method(name = "getBlockWithTxHashes")]
    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> Result<Block, Error>;

    /// Gets block information with full transactions given a block identifier.
    #[method(name = "getBlockWithTxs")]
    fn get_block_w_full_transactions(&self, block_id: BlockId) -> Result<Block, Error>;

    /// Gets the value of the storage at the given address, key, and block.
    #[method(name = "getStorageAt")]
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> Result<StarkFelt, Error>;

    /// Gets the details of a submitted transaction.
    #[method(name = "getTransactionByHash")]
    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<Transaction, Error>;

    /// Gets the details of a transaction by a given block id and index.
    #[method(name = "getTransactionByBlockIdAndIndex")]
    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> Result<Transaction, Error>;

    /// Gets the number of transactions in a block given a block id.
    #[method(name = "getBlockTransactionCount")]
    fn get_block_transaction_count(&self, block_id: BlockId) -> Result<usize, Error>;

    /// Gets the information about the result of executing the requested block.
    #[method(name = "getStateUpdate")]
    fn get_state_update(&self, block_id: BlockId) -> Result<StateUpdate, Error>;

    /// Gets the contract class hash in the given block for the contract deployed at the given
    /// address.
    #[method(name = "getClassHashAt")]
    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, Error>;
}
