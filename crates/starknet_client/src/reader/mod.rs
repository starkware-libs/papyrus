//! This module contains client that can read data from [`Starknet`].
//!
//! [`Starknet`]: https://starknet.io/
// TODO(shahak) Make private once ClientError is refactored and doesn't depend on the reader
// module.
pub(crate) mod objects;
#[cfg(test)]
mod starknet_feeder_gateway_client_test;

use std::collections::HashMap;

use async_trait::async_trait;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use tracing::{debug, instrument};
use url::Url;

pub use crate::reader::objects::block::{Block, GlobalRoot, TransactionReceiptsError};
pub use crate::reader::objects::state::{
    ContractClass, DeclaredClassHashEntry, DeployedContract, ReplacedClass, StateDiff, StateUpdate,
    StorageEntry,
};
#[cfg(doc)]
pub use crate::reader::objects::transaction::TransactionReceipt;
use crate::retry::RetryConfig;
use crate::{
    ClientCreationError, ClientError, ClientResult, StarknetClient, StarknetError,
    StarknetErrorCode,
};

/// A trait describing an object that can communicate with [`Starknet`] and read data from it.
///
/// [`Starknet`]: https://starknet.io/
#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait StarknetReader {
    /// Returns the last block number in the system, returning [`None`] in case there are no blocks
    /// in the system.
    async fn block_number(&self) -> ClientResult<Option<BlockNumber>>;
    /// Returns a [`Block`] corresponding to `block_number`, returning [`None`] in case no such
    /// block exists in the system.
    async fn block(&self, block_number: BlockNumber) -> ClientResult<Option<Block>>;
    /// Returns a [`GenericContractClass`] corresponding to `class_hash`.
    async fn class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ClientResult<Option<GenericContractClass>>;
    /// Returns a [`CasmContractClass`] corresponding to `class_hash`.
    async fn compiled_class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ClientResult<Option<CasmContractClass>>;
    /// Returns a [`starknet_client`][`StateUpdate`] corresponding to `block_number`.
    async fn state_update(&self, block_number: BlockNumber) -> ClientResult<Option<StateUpdate>>;
}

/// A client for the [`Starknet`] feeder gateway.
///
/// [`Starknet`]: https://starknet.io/
pub struct StarknetFeederGatewayClient {
    urls: StarknetUrls,
    client: StarknetClient,
}

#[derive(Clone, Debug)]
struct StarknetUrls {
    get_block: Url,
    get_contract_by_hash: Url,
    get_compiled_class_by_class_hash: Url,
    get_state_update: Url,
}

const GET_BLOCK_URL: &str = "feeder_gateway/get_block";
const GET_CONTRACT_BY_HASH_URL: &str = "feeder_gateway/get_class_by_hash";
const GET_COMPILED_CLASS_BY_CLASS_HASH_URL: &str =
    "feeder_gateway/get_compiled_class_by_class_hash";
const GET_STATE_UPDATE_URL: &str = "feeder_gateway/get_state_update";
const BLOCK_NUMBER_QUERY: &str = "blockNumber";
const LATEST_BLOCK_NUMBER: &str = "latest";
const CLASS_HASH_QUERY: &str = "classHash";

impl StarknetUrls {
    fn new(url_str: &str) -> Result<Self, ClientCreationError> {
        let base_url = Url::parse(url_str)?;
        Ok(StarknetUrls {
            get_block: base_url.join(GET_BLOCK_URL)?,
            get_contract_by_hash: base_url.join(GET_CONTRACT_BY_HASH_URL)?,
            get_compiled_class_by_class_hash: base_url
                .join(GET_COMPILED_CLASS_BY_CLASS_HASH_URL)?,
            get_state_update: base_url.join(GET_STATE_UPDATE_URL)?,
        })
    }
}

impl StarknetFeederGatewayClient {
    pub fn new(
        url_str: &str,
        http_headers: Option<HashMap<String, String>>,
        node_version: &'static str,
        retry_config: RetryConfig,
    ) -> Result<Self, ClientCreationError> {
        Ok(StarknetFeederGatewayClient {
            urls: StarknetUrls::new(url_str)?,
            client: StarknetClient::new(http_headers, node_version, retry_config)?,
        })
    }

    async fn request_with_retry_url(&self, url: Url) -> ClientResult<String> {
        self.client.request_with_retry(self.client.internal_client.get(url)).await
    }

    async fn request_block(
        &self,
        block_number: Option<BlockNumber>,
    ) -> ClientResult<Option<Block>> {
        let mut url = self.urls.get_block.clone();
        let block_number =
            block_number.map(|bn| bn.to_string()).unwrap_or(String::from(LATEST_BLOCK_NUMBER));
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, block_number.as_str());

        let response = self.request_with_retry_url(url).await;
        match response {
            Ok(raw_block) => {
                let block: Block = serde_json::from_str(&raw_block)?;
                Ok(Some(block))
            }
            Err(ClientError::StarknetError(StarknetError {
                code: StarknetErrorCode::BlockNotFound,
                message: _,
            })) => Ok(None),
            Err(err) => {
                debug!("Failed to get block number {:?} from starknet server.", block_number);
                Err(err)
            }
        }
    }
}

#[async_trait]
impl StarknetReader for StarknetFeederGatewayClient {
    #[instrument(skip(self), level = "warn")]
    async fn block_number(&self) -> ClientResult<Option<BlockNumber>> {
        Ok(self.request_block(None).await?.map(|block| block.block_number))
    }

    #[instrument(skip(self), level = "warn")]
    async fn block(&self, block_number: BlockNumber) -> ClientResult<Option<Block>> {
        self.request_block(Some(block_number)).await
    }

    #[instrument(skip(self), level = "warn")]
    async fn class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ClientResult<Option<GenericContractClass>> {
        let mut url = self.urls.get_contract_by_hash.clone();
        let class_hash = serde_json::to_string(&class_hash)?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request_with_retry_url(url).await;
        match response {
            Ok(raw_contract_class) => Ok(Some(serde_json::from_str(&raw_contract_class)?)),
            Err(ClientError::StarknetError(StarknetError {
                code: StarknetErrorCode::UndeclaredClass,
                message: _,
            })) => Ok(None),
            Err(err) => {
                debug!("Failed to get class with hash {:?} from starknet server.", class_hash);
                Err(err)
            }
        }
    }

    #[instrument(skip(self), level = "warn")]
    async fn state_update(&self, block_number: BlockNumber) -> ClientResult<Option<StateUpdate>> {
        let mut url = self.urls.get_state_update.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.to_string());
        let response = self.request_with_retry_url(url).await;
        match response {
            Ok(raw_state_update) => {
                let mut state_update: StateUpdate = serde_json::from_str(&raw_state_update)?;
                // Remove empty storage diffs. The feeder gateway sometimes returns an empty storage
                // diff.
                state_update.state_diff.storage_diffs.retain(|_k, v| !v.is_empty());
                Ok(Some(state_update))
            }
            Err(ClientError::StarknetError(err)) if matches!(err, StarknetError { code, message: _ } if code == StarknetErrorCode::BlockNotFound) => {
                Ok(None)
            }
            Err(err) => {
                debug!(
                    "Failed to get state update for block number {} from starknet server.",
                    block_number
                );
                Err(err)
            }
        }
    }

    #[instrument(skip(self), level = "warn")]
    async fn compiled_class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ClientResult<Option<CasmContractClass>> {
        debug!("Got compiled_class_by_hash {} from starknet server.", class_hash);
        // FIXME: Remove the following default CasmContractClass once integration environment gets
        // regenesissed.
        // Use default value for CasmConractClass that are malformed in the integration environment.
        if [
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x4e70b19333ae94bd958625f7b61ce9eec631653597e68645e13780061b2136c",
                )
                .unwrap(),
            ),
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x6208b3f9f94e6220f3d6a3562fe06a35a66181a202d946c3522fd28eda9ea1b",
                )
                .unwrap(),
            ),
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0xd6916ff38c93f834e7223a95b41d4542152d8288ff388b5d3dcdf8126a784a",
                )
                .unwrap(),
            ),
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x161354521d46ca89a5b64aa41fa4e77ffeadc0f9796272d9b94227dbbb3840e",
                )
                .unwrap(),
            ),
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x6a9eb910b3f83989900c8d65f9d67d67016f2528cc1b834019cf489f4f7d716",
                )
                .unwrap(),
            ),
        ]
        .contains(&class_hash)
        {
            debug!("Using default compiled class for class hash {}.", class_hash);
            return Ok(Some(CasmContractClass::default()));
        }

        let mut url = self.urls.get_compiled_class_by_class_hash.clone();
        let class_hash = serde_json::to_string(&class_hash)?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request_with_retry_url(url).await;
        match response {
            Ok(raw_compiled_class) => Ok(Some(serde_json::from_str(&raw_compiled_class)?)),
            Err(ClientError::StarknetError(StarknetError {
                code: StarknetErrorCode::UndeclaredClass,
                message: _,
            })) => Ok(None),
            Err(err) => {
                debug!(
                    "Failed to get compiled class with hash {:?} from starknet server.",
                    class_hash
                );
                Err(err)
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GenericContractClass {
    Cairo0ContractClass(DeprecatedContractClass),
    Cairo1ContractClass(ContractClass),
}
