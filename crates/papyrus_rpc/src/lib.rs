// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod api;
mod compression_utils;
mod middleware;
mod pending;
mod rpc_metrics;
#[cfg(test)]
mod rpc_test;
mod syncing_state;
#[cfg(test)]
mod test_utils;
mod v0_4;
mod v0_5;
mod v0_6;
mod v0_7;
mod version_config;

use std::collections::BTreeMap;
use std::fmt::Display;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use jsonrpsee::core::RpcResult;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::INTERNAL_ERROR_MSG;
use jsonrpsee::types::ErrorObjectOwned;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_common::BlockHashAndNumber;
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::validators::{validate_ascii, validate_path_exists};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::base_layer::BaseLayerStorageReader;
use papyrus_storage::body::events::EventIndex;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageReader, StorageScope, StorageTxn};
use rpc_metrics::MetricLogger;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockStatus};
use starknet_api::core::ChainId;
use starknet_client::reader::PendingData;
use starknet_client::writer::StarknetGatewayClient;
use starknet_client::RetryConfig;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument};
use validator::Validate;

use crate::api::get_methods_from_supported_apis;
use crate::middleware::{deny_requests_with_unsupported_path, proxy_rpc_request};
use crate::syncing_state::get_last_synced_block;
pub use crate::v0_4::transaction::{
    InvokeTransaction as InvokeTransactionRPC0_4,
    InvokeTransactionV1 as InvokeTransactionV1RPC0_4,
    TransactionVersion1 as TransactionVersion1RPC0_4,
};
pub use crate::v0_4::write_api_result::AddInvokeOkResult as AddInvokeOkResultRPC0_4;

/// Maximum size of a supported transaction body - 10MB.
pub const SERVER_MAX_BODY_SIZE: u32 = 10 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Validate)]
pub struct RpcConfig {
    #[validate(custom = "validate_ascii")]
    pub chain_id: ChainId,
    pub server_address: String,
    pub max_events_chunk_size: usize,
    pub max_events_keys: usize,
    pub collect_metrics: bool,
    pub starknet_url: String,
    pub starknet_gateway_retry_config: RetryConfig,
    #[validate(custom = "validate_path_exists")]
    pub execution_config: PathBuf,
}

impl Default for RpcConfig {
    fn default() -> Self {
        RpcConfig {
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
            execution_config: PathBuf::from("config/execution/mainnet.json"),
        }
    }
}

impl SerializeConfig for RpcConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut self_params_dump = BTreeMap::from_iter([
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "server_address",
                &self.server_address,
                "IP:PORT of the node`s JSON-RPC server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_events_chunk_size",
                &self.max_events_chunk_size,
                "Maximum chunk size supported by the node in get_events requests.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_events_keys",
                &self.max_events_keys,
                "Maximum number of keys supported by the node in get_events requests.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "collect_metrics",
                &self.collect_metrics,
                "If true, collect metrics for the rpc.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "starknet_url",
                &self.starknet_url,
                "URL for communicating with Starknet in write_api methods.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "execution_config",
                &self.execution_config,
                "Path to the execution configuration file.",
                ParamPrivacyInput::Public,
            ),
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

fn internal_server_error_with_msg(err: impl Display) -> ErrorObjectOwned {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    ErrorObjectOwned::owned(InternalError.code(), err.to_string(), None::<()>)
}

fn verify_storage_scope(storage_reader: &StorageReader) -> RpcResult<()> {
    match storage_reader.get_scope() {
        StorageScope::StateOnly => {
            Err(internal_server_error_with_msg("Unsupported method in state-only scope."))
        }
        StorageScope::FullArchive => Ok(()),
    }
}

/// Get the latest block that we've downloaded and that we've downloaded its state diff.
fn get_latest_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, ErrorObjectOwned> {
    Ok(txn.get_state_marker().map_err(internal_server_error)?.prev())
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

#[derive(Clone, Debug, PartialEq)]
struct ContinuationTokenAsStruct(EventIndex);

#[instrument(skip(storage_reader), level = "debug", err)]
pub async fn run_server(
    config: &RpcConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    storage_reader: StorageReader,
    node_version: &'static str,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    let starting_block = get_last_synced_block(storage_reader.clone())?;
    debug!("Starting JSON-RPC.");
    let methods = get_methods_from_supported_apis(
        &config.chain_id,
        config.execution_config.clone().try_into()?,
        storage_reader,
        config.max_events_chunk_size,
        config.max_events_keys,
        starting_block,
        shared_highest_block,
        pending_data,
        pending_classes,
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
        handle = server.start(methods);
    } else {
        let server = server_builder.build(&config.server_address).await?;
        addr = server.local_addr()?;
        handle = server.start(methods);
    }
    info!(local_address = %addr, "JSON-RPC is running.");
    Ok((addr, handle))
}
