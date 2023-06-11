mod api;
mod block;
mod deprecated_contract_class;
#[cfg(test)]
mod gateway_test;
mod middleware;
mod state;
#[cfg(test)]
mod test_utils;
mod transaction;

use std::collections::BTreeMap;
use std::fmt::Display;
use std::net::SocketAddr;

use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
use papyrus_config::{ParamPath, SerdeConfig, SerializedParam, DEFAULT_CHAIN_ID};
use papyrus_storage::body::events::EventIndex;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageReader, StorageTxn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use tracing::{debug, error, info, instrument};

use crate::api::{
    get_methods_from_supported_apis, BlockHashOrNumber, BlockId, ContinuationToken, JsonRpcError,
    Tag,
};
use crate::block::BlockHeader;
use crate::middleware::proxy_request;
use crate::transaction::Transaction;

/// Maximum size of a supported transaction body - 10MB.
pub const SERVER_MAX_BODY_SIZE: u32 = 10 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct GatewayConfig {
    pub chain_id: ChainId,
    pub server_address: String,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig {
            chain_id: ChainId(DEFAULT_CHAIN_ID.to_string()),
            server_address: String::from("0.0.0.0:8080"),
            max_events_chunk_size: 1000,
            max_events_keys: 100,
        }
    }
}

impl SerdeConfig for GatewayConfig {
    fn config_name() -> String {
        String::from("GatewayConfig")
    }

    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            (
                String::from("chain_id"),
                SerializedParam {
                    description: String::from("The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id."),
                    value: json!(self.chain_id),
                }
            ),
            (
                String::from("server_address"),
                SerializedParam {
                    description: String::from("IP:PORT of the node`s JSON-RPC server."),
                    value: json!(self.server_address),
                }
            ),
            (
                String::from("max_events_chunk_size"),
                SerializedParam {
                    description: String::from("Maximum chunk size supported by the node in get_events requests."),
                    value: json!(self.max_events_chunk_size),
                }
            ),
            (
                String::from("max_events_keys"),
                SerializedParam {
                    description: String::from("Maximum number of keys supported by the node in get_events requests."),
                    value: json!(self.max_events_keys),
                }
            ),
        ])
    }
}

impl From<JsonRpcError> for ErrorObjectOwned {
    fn from(err: JsonRpcError) -> Self {
        ErrorObjectOwned::owned(err as i32, err.to_string(), None::<()>)
    }
}

fn internal_server_error(err: impl Display) -> ErrorObjectOwned {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, None::<()>)
}

fn get_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_id: BlockId,
) -> Result<BlockNumber, ErrorObjectOwned> {
    Ok(match block_id {
        BlockId::HashOrNumber(BlockHashOrNumber::Hash(block_hash)) => txn
            .get_block_number_by_hash(&block_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?,
        BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number)) => {
            // Check that the block exists.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;
            if block_number > last_block_number {
                return Err(ErrorObjectOwned::from(JsonRpcError::BlockNotFound));
            }
            block_number
        }
        BlockId::Tag(Tag::Latest) => get_latest_block_number(txn)?
            .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?,
        BlockId::Tag(Tag::Pending) => {
            todo!("Pending tag is not supported yet.")
        }
    })
}

fn get_latest_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, ErrorObjectOwned> {
    Ok(txn.get_header_marker().map_err(internal_server_error)?.prev())
}

fn get_block_header_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockHeader, ErrorObjectOwned> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(BlockHeader::from(header))
}

fn get_block_txs_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<Transaction>, ErrorObjectOwned> {
    let transactions = txn
        .get_block_transactions(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(transactions.into_iter().map(Transaction::from).collect())
}

struct ContinuationTokenAsStruct(EventIndex);

impl ContinuationToken {
    fn parse(&self) -> Result<ContinuationTokenAsStruct, ErrorObjectOwned> {
        let ct = serde_json::from_str(&self.0)
            .map_err(|_| ErrorObjectOwned::from(JsonRpcError::InvalidContinuationToken))?;

        Ok(ContinuationTokenAsStruct(ct))
    }

    fn new(ct: ContinuationTokenAsStruct) -> Result<Self, ErrorObjectOwned> {
        Ok(Self(serde_json::to_string(&ct.0).map_err(internal_server_error)?))
    }
}

#[instrument(skip(storage_reader), level = "debug", err)]
pub async fn run_server(
    config: &GatewayConfig,
    storage_reader: StorageReader,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    debug!("Starting gateway.");
    let server = ServerBuilder::default()
        .max_request_body_size(SERVER_MAX_BODY_SIZE)
        .set_middleware(tower::ServiceBuilder::new().filter_async(proxy_request))
        .build(&config.server_address)
        .await?;
    let addr = server.local_addr()?;
    let methods = get_methods_from_supported_apis(
        &config.chain_id,
        storage_reader,
        config.max_events_chunk_size,
        config.max_events_keys,
    );
    let handle = server.start(methods)?;
    info!(local_address = %addr, "Gateway is running.");
    Ok((addr, handle))
}
