use std::collections::HashSet;
use std::sync::Arc;

use jsonrpsee::{Methods, RpcModule};
use papyrus_common::BlockHashAndNumber;
use papyrus_storage::StorageReader;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::transaction::EventKey;
use starknet_client::writer::StarknetWriter;
use tokio::sync::RwLock;

use crate::v0_3_0::api::api_impl::JsonRpcServerV0_3Impl;
use crate::v0_4_0::api::api_impl::JsonRpcServerV0_4Impl;
use crate::version_config;

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
    #[error("Transaction reverted.")]
    TransactionReverted = 29,
    #[error("Requested page size is too big.")]
    PageSizeTooBig = 31,
    #[error("The supplied continuation token is invalid or unknown.")]
    InvalidContinuationToken = 33,
    #[error("Too many keys provided in a filter.")]
    TooManyKeysInFilter = 34,
    #[error("Contract error.")]
    ContractError = 40,
    // TODO(dvir): delete this when start support pending blocks.
    #[error("Currently, Papyrus doesn't support pending blocks.")]
    PendingBlocksNotSupported = 41,
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

/// Returns a `Methods` object with all the methods from the supported APIs.
/// Whenever adding a new API version we need to add the new version mapping here.
pub fn get_methods_from_supported_apis(
    chain_id: &ChainId,
    storage_reader: StorageReader,
    max_events_chunk_size: usize,
    max_events_keys: usize,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    starknet_writer: Arc<dyn StarknetWriter>,
) -> Methods {
    let mut methods: Methods = Methods::new();
    let server_gen = JsonRpcServerImplGenerator {
        chain_id: chain_id.clone(),
        storage_reader,
        max_events_chunk_size,
        max_events_keys,
        shared_highest_block: shared_highest_block.clone(),
        starknet_writer,
    };
    version_config::VERSION_CONFIG
        .iter()
        .filter_map(|version_config| {
            let (version, version_state) = version_config;
            match version_state {
                version_config::VersionState::Deprecated => None,
                version_config::VersionState::Supported => {
                    let methods = match *version {
                        version_config::VERSION_0_3 => {
                            server_gen.clone().generator::<JsonRpcServerV0_3Impl>()
                        }
                        version_config::VERSION_0_4 => {
                            server_gen.clone().generator::<JsonRpcServerV0_4Impl>()
                        }
                        _ => Methods::new(),
                    };
                    Some(methods)
                }
            }
        })
        .fold(&mut methods, |methods, new_methods| {
            let _res = methods.merge(new_methods);
            methods
        });
    methods
}

pub trait JsonRpcServerImpl: Sized {
    fn new(
        chain_id: ChainId,
        storage_reader: StorageReader,
        max_events_chunk_size: usize,
        max_events_keys: usize,
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        starknet_writer: Arc<dyn StarknetWriter>,
    ) -> Self;

    fn into_rpc_module(self) -> RpcModule<Self>;
}

#[derive(Clone)]
struct JsonRpcServerImplGenerator {
    chain_id: ChainId,
    storage_reader: StorageReader,
    max_events_chunk_size: usize,
    max_events_keys: usize,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    // TODO(shahak): Change this struct to be with a generic type of StarknetWriter.
    starknet_writer: Arc<dyn StarknetWriter>,
}

type JsonRpcServerImplParams = (
    ChainId,
    StorageReader,
    usize,
    usize,
    Arc<RwLock<Option<BlockHashAndNumber>>>,
    Arc<dyn StarknetWriter>,
);

impl JsonRpcServerImplGenerator {
    fn get_params(self) -> JsonRpcServerImplParams {
        (
            self.chain_id,
            self.storage_reader,
            self.max_events_chunk_size,
            self.max_events_keys,
            self.shared_highest_block,
            self.starknet_writer,
        )
    }

    fn generator<T>(self) -> Methods
    where
        T: JsonRpcServerImpl,
    {
        let (
            chain_id,
            storage_reader,
            max_events_chunk_size,
            max_events_keys,
            shared_highest_block,
            starknet_writer,
        ) = self.get_params();
        Into::<Methods>::into(
            T::new(
                chain_id,
                storage_reader,
                max_events_chunk_size,
                max_events_keys,
                shared_highest_block,
                starknet_writer,
            )
            .into_rpc_module(),
        )
    }
}
