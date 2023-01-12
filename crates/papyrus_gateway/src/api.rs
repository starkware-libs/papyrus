use std::collections::HashSet;

use jsonrpsee::core::Error;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{EventKey, TransactionHash, TransactionOffsetInBlock};

use crate::block::Block;
use crate::state::{ContractClass, StateUpdate};
use crate::transaction::{Event, TransactionReceiptWithStatus, TransactionWithType};

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Tag {
    /// The most recent fully constructed block
    #[serde(rename = "latest")]
    Latest,
    /// Currently constructed block
    #[serde(rename = "pending")]
    Pending,
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlockHashOrNumber {
    #[serde(rename = "block_hash")]
    Hash(BlockHash),
    #[serde(rename = "block_number")]
    Number(BlockNumber),
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BlockId {
    HashOrNumber(BlockHashOrNumber),
    Tag(Tag),
}

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum JsonRpcError {
    #[error("There are no blocks.")]
    NoBlocks,
    #[error("Contract not found.")]
    ContractNotFound = 20,
    #[error("Block not found.")]
    BlockNotFound = 24,
    #[error("Transaction hash not found.")]
    TransactionHashNotFound = 25,
    #[error("Invalid transaction index in a block.")]
    InvalidTransactionIndex = 27,
    #[error("Class hash not found.")]
    ClassHashNotFound = 28,
    #[error("Requested page size is too big.")]
    PageSizeTooBig = 31,
    #[error("The supplied continuation token is invalid or unknown.")]
    InvalidContinuationToken = 33,
    #[error("Too many keys provided in a filter.")]
    TooManyKeysInFilter = 34,
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventFilter {
    pub from_block: Option<BlockId>,
    pub to_block: Option<BlockId>,
    pub continuation_token: Option<ContinuationToken>,
    pub chunk_size: usize,
    pub address: Option<ContractAddress>,
    #[serde(default)]
    pub keys: Vec<HashSet<EventKey>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct ContinuationToken(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventsChunk {
    pub events: Vec<Event>,
    pub continuation_token: Option<ContinuationToken>,
}

#[rpc(server, client, namespace = "starknet")]
pub trait JsonRpc {
    /// Gets the most recent accepted block number.
    #[method(name = "blockNumber")]
    fn block_number(&self) -> Result<BlockNumber, Error>;

    /// Gets the most recent accepted block hash and number.
    #[method(name = "blockHashAndNumber")]
    fn block_hash_and_number(&self) -> Result<BlockHashAndNumber, Error>;

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
    ) -> Result<TransactionWithType, Error>;

    /// Gets the details of a transaction by a given block id and index.
    #[method(name = "getTransactionByBlockIdAndIndex")]
    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> Result<TransactionWithType, Error>;

    /// Gets the number of transactions in a block given a block id.
    #[method(name = "getBlockTransactionCount")]
    fn get_block_transaction_count(&self, block_id: BlockId) -> Result<usize, Error>;

    /// Gets the information about the result of executing the requested block.
    #[method(name = "getStateUpdate")]
    fn get_state_update(&self, block_id: BlockId) -> Result<StateUpdate, Error>;

    /// Gets the transaction receipt by the transaction hash.
    #[method(name = "getTransactionReceipt")]
    fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionReceiptWithStatus, Error>;

    /// Gets the contract class definition associated with the given hash.
    #[method(name = "getClass")]
    fn get_class(&self, block_id: BlockId, class_hash: ClassHash) -> Result<ContractClass, Error>;

    /// Gets the contract class definition in the given block at the given address.
    #[method(name = "getClassAt")]
    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ContractClass, Error>;

    /// Gets the contract class hash in the given block for the contract deployed at the given
    /// address.
    #[method(name = "getClassHashAt")]
    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, Error>;

    /// Gets the nonce associated with the given address in the given block.
    #[method(name = "getNonce")]
    fn get_nonce(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<Nonce, Error>;

    /// Returns the currently configured StarkNet chain id.
    #[method(name = "chainId")]
    fn chain_id(&self) -> Result<String, Error>;

    /// Returns all events matching the given filter.
    #[method(name = "getEvents")]
    fn get_events(&self, filter: EventFilter) -> Result<EventsChunk, Error>;
}
