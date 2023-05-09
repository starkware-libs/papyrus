pub mod v0_3_0;
#[cfg(test)]
mod v0_3_0_test;
pub mod version_config;
#[cfg(test)]
mod version_config_test;

use std::collections::HashSet;

use jsonrpsee::core::server::rpc_module::Methods;
use papyrus_storage::StorageReader;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::transaction::EventKey;

use self::v0_3_0::{JsonRpcServerV0_3_0Impl, JsonRpcV0_3_0Server};
use crate::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use crate::state::ContractClass;
use crate::transaction::Event;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GatewayContractClass {
    Cairo0(DeprecatedContractClass),
    Sierra(ContractClass),
}
