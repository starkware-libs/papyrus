//! Client implementation for [`starknet`] gateway.
//!
//! [`starknet`]: https://starknet.io/

mod objects;
pub mod retry;
#[cfg(test)]
mod starknet_client_test;
#[cfg(test)]
mod test_utils;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use reqwest::header::HeaderMap;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::StarknetApiError;
use tracing::debug;
use url::Url;

pub use self::objects::block::{
    Block, ContractClass, DeployedContract, GlobalRoot, StateDiff, StateUpdate, StorageEntry,
    TransactionReceiptsError,
};
use self::retry::Retry;
pub use self::retry::RetryConfig;
#[cfg(doc)]
pub use crate::objects::transaction::TransactionReceipt;

/// A [`Result`] in which the error is a [`ClientError`].
pub type ClientResult<T> = Result<T, ClientError>;

/// Methods for querying starknet.
#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait StarknetClientTrait {
    /// Returns the last block number in the system, returning [`None`] in case there are no blocks
    /// in the system.
    async fn block_number(&self) -> ClientResult<Option<BlockNumber>>;
    /// Returns a [`Block`] corresponding to `block_number`, returning [`None`] in case no such
    /// block exists in the system.
    async fn block(&self, block_number: BlockNumber) -> ClientResult<Option<Block>>;
    /// Returns a [`ContractClass`] corresponding to `class_hash`.
    async fn class_by_hash(&self, class_hash: ClassHash) -> ClientResult<Option<ContractClass>>;
    /// Returns a [`starknet_clinet`][`StateUpdate`] corresponding to `block_number`.
    async fn state_update(&self, block_number: BlockNumber) -> ClientResult<Option<StateUpdate>>;
}

/// A starknet client.
pub struct StarknetClient {
    urls: StarknetUrls,
    http_headers: HeaderMap,
    internal_client: Client,
    retry_config: RetryConfig,
}

#[derive(Clone, Debug)]
struct StarknetUrls {
    get_block: Url,
    get_contract_by_hash: Url,
    get_state_update: Url,
}

/// Error codes returned by the starknet gateway.
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum StarknetErrorCode {
    #[serde(rename = "StarknetErrorCode.BLOCK_NOT_FOUND")]
    BlockNotFound = 0,
    #[serde(rename = "StarknetErrorCode.OUT_OF_RANGE_CLASS_HASH")]
    OutOfRangeClassHash = 26,
    #[serde(rename = "StarkErrorCode.MALFORMED_REQUEST")]
    MalformedRequest = 32,
    #[serde(rename = "StarknetErrorCode.UNDECLARED_CLASS")]
    UndeclaredClass = 44,
}

/// A client error wrapping error codes returned by the starknet gateway.
#[derive(thiserror::Error, Debug, Deserialize, Serialize)]
pub struct StarknetError {
    pub code: StarknetErrorCode,
    pub message: String,
}

/// Errors that might be encountered while creating the client.
#[derive(thiserror::Error, Debug)]
pub enum ClientCreationError {
    #[error(transparent)]
    BadUrl(#[from] url::ParseError),
    #[error(transparent)]
    BuildError(#[from] reqwest::Error),
    #[error(transparent)]
    HttpHeaderError(#[from] http::Error),
}

/// Errors that might be solved by retrying mechanism.
#[derive(Debug, Eq, PartialEq)]
pub enum RetryErrorCode {
    Redirect,
    Timeout,
    TooManyRequests,
    ServiceUnavailable,
    Disconnect,
}

/// Errors that may be returned by the client.
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    /// A client error representing bad status http responses.
    #[error("Bad response status code: {:?} message: {:?}.", code, message)]
    BadResponseStatus { code: StatusCode, message: String },
    /// A client error representing http request errors.
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    /// A client error representing errors that might be solved by retrying mechanism.
    #[error("Retry error code: {:?}, message: {:?}.", code, message)]
    RetryError { code: RetryErrorCode, message: String },
    /// A client error representing deserialisation errors.
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    /// A client error representing errors from [`starknet_api`].
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    /// A client error representing errors returned by the starknet client.
    #[error(transparent)]
    StarknetError(#[from] StarknetError),
    /// A client error representing transaction receipts errors.
    #[error(transparent)]
    TransactionReceiptsError(#[from] TransactionReceiptsError),
}

const GET_BLOCK_URL: &str = "feeder_gateway/get_block";
const GET_CONTRACT_BY_HASH_URL: &str = "feeder_gateway/get_class_by_hash";
const GET_STATE_UPDATE_URL: &str = "feeder_gateway/get_state_update";
const BLOCK_NUMBER_QUERY: &str = "blockNumber";
const CLASS_HASH_QUERY: &str = "classHash";

impl StarknetUrls {
    fn new(url_str: &str) -> Result<Self, ClientCreationError> {
        let base_url = Url::parse(url_str)?;
        Ok(StarknetUrls {
            get_block: base_url.join(GET_BLOCK_URL)?,
            get_contract_by_hash: base_url.join(GET_CONTRACT_BY_HASH_URL)?,
            get_state_update: base_url.join(GET_STATE_UPDATE_URL)?,
        })
    }
}

impl Display for StarknetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl StarknetClient {
    /// Creates a new client for a starknet gateway at `url_str` with retry_config [`RetryConfig`].
    pub fn new(
        url_str: &str,
        http_headers: Option<HashMap<String, String>>,
        retry_config: RetryConfig,
    ) -> Result<StarknetClient, ClientCreationError> {
        let header_map = match http_headers {
            Some(inner) => (&inner).try_into()?,
            None => HeaderMap::new(),
        };
        Ok(StarknetClient {
            urls: StarknetUrls::new(url_str)?,
            http_headers: header_map,
            internal_client: Client::builder().build()?,
            retry_config,
        })
    }

    fn get_retry_error_code(err: &ClientError) -> Option<RetryErrorCode> {
        match err {
            ClientError::BadResponseStatus { code, message: _ } => match *code {
                StatusCode::TEMPORARY_REDIRECT => Some(RetryErrorCode::Redirect),
                StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => {
                    Some(RetryErrorCode::Timeout)
                }
                StatusCode::TOO_MANY_REQUESTS => Some(RetryErrorCode::TooManyRequests),
                StatusCode::SERVICE_UNAVAILABLE => Some(RetryErrorCode::ServiceUnavailable),
                _ => None,
            },

            ClientError::RequestError(internal_err) => {
                if internal_err.is_timeout() {
                    Some(RetryErrorCode::Timeout)
                } else if internal_err.is_connect() {
                    Some(RetryErrorCode::Disconnect)
                } else if internal_err.is_redirect() {
                    Some(RetryErrorCode::Redirect)
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    fn should_retry(err: &ClientError) -> bool {
        Self::get_retry_error_code(err).is_some()
    }

    async fn request_with_retry(&self, url: Url) -> Result<String, ClientError> {
        Retry::new(&self.retry_config)
            .start_with_condition(|| self.request(url.clone()), Self::should_retry)
            .await
            .map_err(|err| {
                Self::get_retry_error_code(&err)
                    .map(|code| ClientError::RetryError { code, message: err.to_string() })
                    .unwrap_or(err)
            })
    }

    async fn request(&self, url: Url) -> ClientResult<String> {
        let res = self.internal_client.get(url).headers(self.http_headers.clone()).send().await;
        let (code, message) = match res {
            Ok(response) => (response.status(), response.text().await?),
            Err(err) => {
                let msg = err.to_string();
                (err.status().ok_or(err)?, msg)
            }
        };
        match code {
            StatusCode::OK => Ok(message),
            StatusCode::INTERNAL_SERVER_ERROR => {
                let starknet_error: StarknetError = serde_json::from_str(&message)?;
                Err(ClientError::StarknetError(starknet_error))
            }
            _ => Err(ClientError::BadResponseStatus { code, message }),
        }
    }

    async fn request_block(
        &self,
        block_number: Option<BlockNumber>,
    ) -> ClientResult<Option<Block>> {
        let mut url = self.urls.get_block.clone();
        if let Some(block_number) = block_number {
            url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.to_string());
        }

        let response = self.request_with_retry(url).await;
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
impl StarknetClientTrait for StarknetClient {
    async fn block_number(&self) -> ClientResult<Option<BlockNumber>> {
        Ok(self.request_block(None).await?.map(|block| block.block_number))
    }

    async fn block(&self, block_number: BlockNumber) -> ClientResult<Option<Block>> {
        self.request_block(Some(block_number)).await
    }

    async fn class_by_hash(&self, class_hash: ClassHash) -> ClientResult<Option<ContractClass>> {
        let mut url = self.urls.get_contract_by_hash.clone();
        let class_hash = serde_json::to_string(&class_hash)?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request_with_retry(url).await;
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

    async fn state_update(&self, block_number: BlockNumber) -> ClientResult<Option<StateUpdate>> {
        let mut url = self.urls.get_state_update.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.to_string());
        let response = self.request_with_retry(url).await;
        match response {
            Ok(raw_state_update) => {
                let state_update: StateUpdate = serde_json::from_str(&raw_state_update)?;
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
}
