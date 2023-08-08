mod api;
mod block;
mod gateway_metrics;
#[cfg(test)]
mod gateway_test;
mod middleware;
mod syncing_state;
#[cfg(test)]
mod test_utils;
mod transaction;
mod v0_3_0;
mod v0_4_0;
mod version_config;
#[cfg(test)]
mod version_config_test;

use std::collections::BTreeMap;
use std::fmt::Display;
use std::net::SocketAddr;
use std::sync::Arc;

use gateway_metrics::MetricLogger;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
use papyrus_common::BlockHashAndNumber;
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, SerializedParam};
use papyrus_storage::base_layer::BaseLayerStorageReader;
use papyrus_storage::body::events::EventIndex;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageReader, StorageTxn};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockStatus};
use starknet_api::core::ChainId;
use starknet_client::writer::StarknetGatewayClient;
use starknet_client::RetryConfig;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument};

use crate::api::{
    get_methods_from_supported_apis, BlockHashOrNumber, BlockId, ContinuationToken, JsonRpcError,
    Tag,
};
use crate::middleware::{deny_requests_with_unsupported_path, proxy_rpc_request};

/// Maximum size of a supported transaction body - 10MB.
pub const SERVER_MAX_BODY_SIZE: u32 = 10 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct GatewayConfig {
    pub chain_id: ChainId,
    pub server_address: String,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    pub collect_metrics: bool,
    pub starknet_url: String,
    pub starknet_gateway_retry_config: RetryConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig {
            chain_id: ChainId("SN_MAIN".to_string()),
            server_address: String::from("0.0.0.0:8080"),
            max_events_chunk_size: 1000,
            max_events_keys: 100,
            collect_metrics: false,
            starknet_url: String::from("https://alpha-mainnet.starknet.io/"),
            starknet_gateway_retry_config: RetryConfig {
                retry_base_millis: 50,
                retry_max_delay_millis: 1000,
                max_retries: 5,
            },
        }
    }
}

impl SerializeConfig for GatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut self_params_dump = BTreeMap::from_iter([
            ser_param("chain_id", &self.chain_id, "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id."),
            ser_param("server_address", &self.server_address, "IP:PORT of the node`s JSON-RPC server."),
            ser_param("max_events_chunk_size", &self.max_events_chunk_size, "Maximum chunk size supported by the node in get_events requests."),
            ser_param("max_events_keys", &self.max_events_keys, "Maximum number of keys supported by the node in get_events requests."),
            ser_param("collect_metrics", &self.collect_metrics, "If true, collect metrics for the gateway."),
            ser_param("starknet_url", &self.starknet_url, "URL for communicating with Starknet in write_api methods."),
        ]);
        let mut retry_config_dump = append_sub_config_name(
            self.starknet_gateway_retry_config.dump(),
            "starknet_gateway_retry_config",
        );
        for param in retry_config_dump.values_mut() {
            param.description = format!(
                "For communicating with Starknet gateway, {}{}",
                param.description[0..1].to_lowercase(),
                &param.description[1..]
            );
        }
        self_params_dump.append(&mut retry_config_dump);
        self_params_dump
    }
}

fn internal_server_error(err: impl Display) -> ErrorObjectOwned {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    ErrorObjectOwned::owned(InternalError.code(), INTERNAL_ERROR_MSG, None::<()>)
}

fn get_latest_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, ErrorObjectOwned> {
    Ok(txn.get_header_marker().map_err(internal_server_error)?.prev())
}

fn get_block_status<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockStatus, ErrorObjectOwned> {
    let base_layer_tip = txn.get_base_layer_block_marker().map_err(internal_server_error)?;
    let status = if block_number < base_layer_tip {
        BlockStatus::AcceptedOnL1
    } else {
        BlockStatus::AcceptedOnL2
    };

    Ok(status)
}
struct ContinuationTokenAsStruct(EventIndex);

#[instrument(skip(storage_reader), level = "debug", err)]
pub async fn run_server(
    config: &GatewayConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    storage_reader: StorageReader,
    node_version: &'static str,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    debug!("Starting gateway.");
    let methods = get_methods_from_supported_apis(
        &config.chain_id,
        storage_reader,
        config.max_events_chunk_size,
        config.max_events_keys,
        shared_highest_block,
        Arc::new(StarknetGatewayClient::new(
            &config.starknet_url,
            node_version,
            config.starknet_gateway_retry_config,
        )?),
    );
    let addr;
    let handle;
    let server_builder =
        ServerBuilder::default().max_request_body_size(SERVER_MAX_BODY_SIZE).set_middleware(
            tower::ServiceBuilder::new()
                .filter_async(deny_requests_with_unsupported_path)
                .filter_async(proxy_rpc_request),
        );

    if config.collect_metrics {
        let server = server_builder
            .set_logger(MetricLogger::new(&methods))
            .build(&config.server_address)
            .await?;
        addr = server.local_addr()?;
        handle = server.start(methods)?;
    } else {
        let server = server_builder.build(&config.server_address).await?;
        addr = server.local_addr()?;
        handle = server.start(methods)?;
    }
    info!(local_address = %addr, "Gateway is running.");
    Ok((addr, handle))
}
