//! This module contains client that can read data from [`Starknet`].
//!
//! [`Starknet`]: https://starknet.io/

pub mod objects;
#[cfg(test)]
mod starknet_feeder_gateway_client_test;

use std::collections::HashMap;

use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_common::pending_classes::ApiContractClass;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, SequencerPublicKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::transaction::TransactionHash;
use starknet_api::StarknetApiError;
use tracing::{debug, error, instrument};
use url::Url;

pub use crate::reader::objects::block::{
    BlockOrDeprecated,
    BlockSignatureData,
    BlockSignatureMessage,
    TransactionReceiptsError,
};
pub use crate::reader::objects::pending_data::PendingData;
pub use crate::reader::objects::state::{
    ContractClass,
    DeclaredClassHashEntry,
    DeployedContract,
    ReplacedClass,
    StateDiff,
    StateUpdate,
    StorageEntry,
};
#[cfg(doc)]
pub use crate::reader::objects::transaction::TransactionReceipt;
use crate::retry::RetryConfig;
use crate::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};
use crate::{ClientCreationError, ClientError, StarknetClient};

/// Errors that may be returned from a reader client.
#[derive(thiserror::Error, Debug)]
pub enum ReaderClientError {
    /// A client error representing errors from the base StarknetClient.
    #[error(transparent)]
    ClientError(#[from] ClientError),
    /// A client error representing deserialization errors.
    /// Note: [`ClientError`] contains SerdeError as well. The difference is that this variant is
    /// responsible for serde errors coming from [`StarknetReader`] and ClientError::SerdeError
    /// is responsible for serde errors coming from StarknetClient.
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    /// A client error representing errors from [`starknet_api`].
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    /// A client error representing transaction receipts errors.
    #[error(transparent)]
    TransactionReceiptsError(#[from] TransactionReceiptsError),
    #[error("Invalid transaction: {:?}, error: {:?}.", tx_hash, msg)]
    BadTransaction { tx_hash: TransactionHash, msg: String },
}

pub type ReaderClientResult<T> = Result<T, ReaderClientError>;

/// A trait describing an object that can communicate with [`Starknet`] and read data from it.
///
/// [`Starknet`]: https://starknet.io/
#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait StarknetReader {
    /// Returns the last block in the system, returning [`None`] in case there are no blocks in the
    /// system.
    async fn latest_block(&self) -> ReaderClientResult<Option<BlockOrDeprecated>>;
    /// Returns a [`BlockOrDeprecated`] corresponding to `block_number`, returning [`None`] in case
    /// no such block exists in the system.
    async fn block(
        &self,
        block_number: BlockNumber,
    ) -> ReaderClientResult<Option<BlockOrDeprecated>>;
    /// Returns a [`GenericContractClass`] corresponding to `class_hash`.
    async fn class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ReaderClientResult<Option<GenericContractClass>>;
    /// Returns a [`CasmContractClass`] corresponding to `class_hash`.
    async fn compiled_class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ReaderClientResult<Option<CasmContractClass>>;
    /// Returns a [`starknet_client`][`StateUpdate`] corresponding to `block_number`.
    async fn state_update(
        &self,
        block_number: BlockNumber,
    ) -> ReaderClientResult<Option<StateUpdate>>;

    // TODO(dvir): delete this when stop supporting pending data.
    /// Returns pending [`starknet_client`][`PendingData`].
    async fn pending_data(&self) -> ReaderClientResult<Option<PendingData>>;

    // Returns true if the reader is alive.
    async fn is_alive(&self) -> bool;

    async fn block_signature(
        &self,
        block_number: BlockNumber,
    ) -> ReaderClientResult<Option<BlockSignatureData>>;

    async fn sequencer_pub_key(&self) -> ReaderClientResult<SequencerPublicKey>;
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
    get_pending_data: Url,
    feeder_gateway_is_alive: Url,
    get_block_signature: Url,
    get_sequencer_pub_key: Url,
}

const GET_BLOCK_URL: &str = "feeder_gateway/get_block";
const GET_CONTRACT_BY_HASH_URL: &str = "feeder_gateway/get_class_by_hash";
const GET_COMPILED_CLASS_BY_CLASS_HASH_URL: &str =
    "feeder_gateway/get_compiled_class_by_class_hash";
const GET_STATE_UPDATE_URL: &str = "feeder_gateway/get_state_update";
const BLOCK_NUMBER_QUERY: &str = "blockNumber";
const LATEST_BLOCK_NUMBER: &str = "latest";
const CLASS_HASH_QUERY: &str = "classHash";
const PENDING_BLOCK_ID: &str = "pending";
const INCLUDE_BLOCK: &str = "includeBlock";
const FEEDER_GATEWAY_IS_ALIVE: &str = "feeder_gateway/is_alive";
const FEEDER_GATEWAY_ALIVE_RESPONSE: &str = "FeederGateway is alive!";
const GET_BLOCK_SIGNATURE_URL: &str = "feeder_gateway/get_signature";
const GET_SEQUENCER_PUB_KEY_URL: &str = "feeder_gateway/get_public_key";

impl StarknetUrls {
    fn new(url_str: &str) -> Result<Self, ClientCreationError> {
        let base_url = Url::parse(url_str)?;
        Ok(StarknetUrls {
            get_block: base_url.join(GET_BLOCK_URL)?,
            // Query the pending block since the feeder-gateway uses the `latest` block by default.
            get_contract_by_hash: base_url
                .join(GET_CONTRACT_BY_HASH_URL)?
                .query_pairs_mut()
                .append_pair(BLOCK_NUMBER_QUERY, PENDING_BLOCK_ID)
                .finish()
                .clone(),
            // Query the pending block since the feeder-gateway uses the `latest` block by default.
            get_compiled_class_by_class_hash: base_url
                .join(GET_COMPILED_CLASS_BY_CLASS_HASH_URL)?
                .query_pairs_mut()
                .append_pair(BLOCK_NUMBER_QUERY, PENDING_BLOCK_ID)
                .finish()
                .clone(),
            get_state_update: base_url.join(GET_STATE_UPDATE_URL)?,
            get_pending_data: base_url
                .join(GET_STATE_UPDATE_URL)?
                .query_pairs_mut()
                .append_pair(BLOCK_NUMBER_QUERY, PENDING_BLOCK_ID)
                .append_pair(INCLUDE_BLOCK, "true")
                .finish()
                .clone(),
            feeder_gateway_is_alive: base_url.join(FEEDER_GATEWAY_IS_ALIVE)?,
            get_block_signature: base_url.join(GET_BLOCK_SIGNATURE_URL)?,
            get_sequencer_pub_key: base_url.join(GET_SEQUENCER_PUB_KEY_URL)?,
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

    async fn request_with_retry_url(&self, url: Url) -> ReaderClientResult<String> {
        self.client
            .request_with_retry(self.client.internal_client.get(url))
            .await
            .map_err(Into::<ReaderClientError>::into)
    }

    async fn request_block(
        &self,
        block_number: Option<BlockNumber>,
    ) -> ReaderClientResult<Option<BlockOrDeprecated>> {
        let mut url = self.urls.get_block.clone();
        let block_number =
            block_number.map(|bn| bn.to_string()).unwrap_or(String::from(LATEST_BLOCK_NUMBER));
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, block_number.as_str());

        let response = self.request_with_retry_url(url).await;
        load_object_from_response(
            response,
            Some(KnownStarknetErrorCode::BlockNotFound),
            format!("Failed to get block number {block_number:?} from starknet server."),
        )
    }
}

#[async_trait]
impl StarknetReader for StarknetFeederGatewayClient {
    #[instrument(skip(self), level = "debug")]
    async fn latest_block(&self) -> ReaderClientResult<Option<BlockOrDeprecated>> {
        Ok(self.request_block(None).await?)
    }

    #[instrument(skip(self), level = "debug")]
    async fn block(
        &self,
        block_number: BlockNumber,
    ) -> ReaderClientResult<Option<BlockOrDeprecated>> {
        self.request_block(Some(block_number)).await
    }

    #[instrument(skip(self), level = "debug")]
    async fn class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ReaderClientResult<Option<GenericContractClass>> {
        let mut url = self.urls.get_contract_by_hash.clone();
        let class_hash_result = serde_json::to_string(&class_hash);
        if class_hash_result.is_err() {
            error!("Failed to serialize {class_hash:?}");
        }
        let class_hash = class_hash_result?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request_with_retry_url(url).await;
        load_object_from_response(
            response,
            Some(KnownStarknetErrorCode::UndeclaredClass),
            format!("Failed to get class with hash {class_hash:?} from starknet server."),
        )
    }

    #[instrument(skip(self), level = "debug")]
    async fn state_update(
        &self,
        block_number: BlockNumber,
    ) -> ReaderClientResult<Option<StateUpdate>> {
        let mut url = self.urls.get_state_update.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.to_string());
        let response = self.request_with_retry_url(url).await;
        load_object_from_response(
            response,
            Some(KnownStarknetErrorCode::BlockNotFound),
            format!(
                "Failed to get state update for block number {block_number} from starknet server."
            ),
        )
    }

    #[instrument(skip(self), level = "debug")]
    async fn compiled_class_by_hash(
        &self,
        class_hash: ClassHash,
    ) -> ReaderClientResult<Option<CasmContractClass>> {
        debug!("Got compiled_class_by_hash {} from starknet server.", class_hash);
        // FIXME: Remove the following default CasmContractClass once integration environment gets
        // regenesissed.
        // Use default value for CasmConractClass that are malformed in the integration environment.
        // TODO: Make this array a const.
        if [
            #[allow(clippy::unwrap_used)]
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x4e70b19333ae94bd958625f7b61ce9eec631653597e68645e13780061b2136c",
                )
                .unwrap(),
            ),
            #[allow(clippy::unwrap_used)]
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x6208b3f9f94e6220f3d6a3562fe06a35a66181a202d946c3522fd28eda9ea1b",
                )
                .unwrap(),
            ),
            #[allow(clippy::unwrap_used)]
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0xd6916ff38c93f834e7223a95b41d4542152d8288ff388b5d3dcdf8126a784a",
                )
                .unwrap(),
            ),
            #[allow(clippy::unwrap_used)]
            ClassHash(
                starknet_api::hash::StarkFelt::try_from(
                    "0x161354521d46ca89a5b64aa41fa4e77ffeadc0f9796272d9b94227dbbb3840e",
                )
                .unwrap(),
            ),
            #[allow(clippy::unwrap_used)]
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
        let class_hash_result = serde_json::to_string(&class_hash);
        if class_hash_result.is_err() {
            error!("Failed to serialize {class_hash:?}");
        }
        let class_hash = class_hash_result?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request_with_retry_url(url).await;
        load_object_from_response(
            response,
            Some(KnownStarknetErrorCode::UndeclaredClass),
            format!("Failed to get compiled class with hash {class_hash:?} from starknet server."),
        )
    }

    #[instrument(skip(self), level = "debug")]
    async fn pending_data(&self) -> ReaderClientResult<Option<PendingData>> {
        let response = self.request_with_retry_url(self.urls.get_pending_data.clone()).await;
        load_object_from_response(
            response,
            Some(KnownStarknetErrorCode::BlockNotFound),
            "Failed to get pending data from starknet server.".to_string(),
        )
    }

    async fn is_alive(&self) -> bool {
        let url = self.urls.feeder_gateway_is_alive.clone();
        let response = self.request_with_retry_url(url).await;
        let expected_response = FEEDER_GATEWAY_ALIVE_RESPONSE.to_string();
        response.is_ok_and(|response| response == expected_response)
    }

    #[instrument(skip(self), level = "debug")]
    async fn block_signature(
        &self,
        block_number: BlockNumber,
    ) -> ReaderClientResult<Option<BlockSignatureData>> {
        let mut url = self.urls.get_block_signature.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.to_string());
        let response = self.request_with_retry_url(url).await;
        load_object_from_response(
            response,
            Some(KnownStarknetErrorCode::BlockNotFound),
            format!("Failed to get signature for block {block_number:?} from starknet server."),
        )
    }

    #[instrument(skip(self), level = "debug")]
    async fn sequencer_pub_key(&self) -> ReaderClientResult<SequencerPublicKey> {
        let response = self.request_with_retry_url(self.urls.get_sequencer_pub_key.clone()).await;
        load_object_from_response(
            response,
            None,
            "Failed to get sequencer public key from starknet server.".to_string(),
        )
        .map(|option| option.expect("Sequencer public key should not be None."))
    }
}

/// Load an object from a json string response. If there was a StarknetError with
/// `none_error_code`, return None. If there was a different error, log `error_message`.
fn load_object_from_response<Object: for<'a> Deserialize<'a>>(
    response: ReaderClientResult<String>,
    none_error_code: Option<KnownStarknetErrorCode>,
    error_message: String,
) -> ReaderClientResult<Option<Object>> {
    match (response, none_error_code) {
        (Ok(raw_object), _) => {
            let result = serde_json::from_str(&raw_object);
            if let Err(err) = &result {
                error!("Failed to deserialize {raw_object:?}. Error: {err}");
            }
            Ok(Some(result?))
        }
        (
            Err(ReaderClientError::ClientError(ClientError::StarknetError(StarknetError {
                code: StarknetErrorCode::KnownErrorCode(error_code),
                message: _,
            }))),
            Some(none_error_code),
        ) if error_code == none_error_code => Ok(None),
        (Err(err), _) => {
            debug!(error_message);
            Err(err)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GenericContractClass {
    Cairo0ContractClass(DeprecatedContractClass),
    Cairo1ContractClass(ContractClass),
}

impl From<GenericContractClass> for ApiContractClass {
    fn from(value: GenericContractClass) -> Self {
        match value {
            GenericContractClass::Cairo0ContractClass(class) => {
                Self::DeprecatedContractClass(class)
            }
            GenericContractClass::Cairo1ContractClass(class) => Self::ContractClass(class.into()),
        }
    }
}
