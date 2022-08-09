mod objects;
pub mod retry;
#[cfg(test)]
mod starknet_client_test;
#[cfg(test)]
mod test_utils;

use std::fmt::{self, Display, Formatter};

use async_trait::async_trait;
use log::error;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockNumber, ClassHash, ContractClass};
use url::Url;

pub use self::objects::block::{client_to_starknet_api_storage_diff, Block, BlockStateUpdate};
pub use self::retry::{Retry, RetryConfig};

pub type ClientResult<T> = Result<T, ClientError>;

#[async_trait]
pub trait StarknetClientTrait {
    async fn block_number(&self) -> ClientResult<Option<BlockNumber>>;
    async fn block(&self, block_number: BlockNumber) -> ClientResult<Option<Block>>;
    async fn class_by_hash(&self, class_hash: ClassHash) -> ClientResult<ContractClass>;
    async fn state_update(&self, block_number: BlockNumber) -> ClientResult<BlockStateUpdate>;
}

pub struct StarknetClient {
    urls: StarknetUrls,
    internal_client: Client,
    retry_config: RetryConfig,
}

#[derive(Clone, Debug)]
struct StarknetUrls {
    get_block: Url,
    get_contract_by_hash: Url,
    get_state_update: Url,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum StarknetErrorCode {
    #[serde(rename = "StarknetErrorCode.BLOCK_NOT_FOUND")]
    BlockNotFound,
    // TODO(anatg): Add more error codes as they become relevant.
}

#[derive(thiserror::Error, Debug, Deserialize, Serialize)]
pub struct StarknetError {
    pub code: StarknetErrorCode,
    pub message: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ClientCreationError {
    #[error(transparent)]
    BadUrl(#[from] url::ParseError),
    #[error(transparent)]
    BuildError(#[from] reqwest::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RetryErrorCode {
    Redirect,
    Timeout,
    TooManyRequests,
    ServiceUnavailable,
    Disconnect,
}

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("Bad status error code: {:?} message: {:?}.", code, message)]
    BadStatusError { code: StatusCode, message: String },
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Retry error code: {:?}, message: {:?}.", code, message)]
    RetryError { code: RetryErrorCode, message: String },
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
    #[error(transparent)]
    StarknetError(#[from] StarknetError),
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
        write!(f, "{:?}", self)
    }
}

impl StarknetClient {
    pub fn new(
        url_str: &str,
        retry_config: RetryConfig,
    ) -> Result<StarknetClient, ClientCreationError> {
        Ok(StarknetClient {
            urls: StarknetUrls::new(url_str)?,
            internal_client: Client::builder().build()?,
            retry_config,
        })
    }

    fn get_retry_error(err: &ClientError) -> Option<RetryErrorCode> {
        match err {
            ClientError::BadStatusError { code, message: _ } => match *code {
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
        Self::get_retry_error(err).is_some()
    }

    async fn request_with_retry(&self, url: Url) -> Result<String, ClientError> {
        Retry::new(&self.retry_config)
            .start_with_condition(|| self.request(url.clone()), Self::should_retry)
            .await
            .map_err(|err| {
                Self::get_retry_error(&err)
                    .map(|code| ClientError::RetryError { code, message: err.to_string() })
                    .unwrap_or(err)
            })
    }

    async fn request(&self, url: Url) -> ClientResult<String> {
        let res = self.internal_client.get(url).send().await;
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
                error!(
                    "Starknet server responded with an internal server error: {}.",
                    starknet_error
                );
                Err(ClientError::StarknetError(starknet_error))
            }
            _ => {
                // TODO(dan): consider logging as info instead.
                error!("Bad status error code: {:?}, message: {:?}.", code, message);
                Err(ClientError::BadStatusError { code, message })
            }
        }
    }

    async fn request_block(
        &self,
        block_number: Option<BlockNumber>,
    ) -> ClientResult<Option<Block>> {
        let mut url = self.urls.get_block.clone();
        if let Some(block_number) = block_number {
            url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.0.to_string());
        }

        let response = self.request_with_retry(url).await;
        match response {
            Ok(raw_block) => {
                let block: Block = serde_json::from_str(&raw_block)?;
                Ok(Some(block))
            }
            Err(err) => match err {
                ClientError::StarknetError(sn_err) => {
                    let StarknetError { code, message } = sn_err;
                    // If there are no blocks in Starknet, return None.
                    if code == StarknetErrorCode::BlockNotFound {
                        Ok(None)
                    } else {
                        Err(ClientError::StarknetError(StarknetError { code, message }))
                    }
                }
                _ => Err(err),
            },
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

    async fn class_by_hash(&self, class_hash: ClassHash) -> ClientResult<ContractClass> {
        let mut url = self.urls.get_contract_by_hash.clone();
        let class_hash = serde_json::to_string(&class_hash)?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request_with_retry(url).await;
        match response {
            Ok(raw_contract_class) => Ok(serde_json::from_str(&raw_contract_class)?),
            Err(err) => {
                error!("{}", err);
                Err(err)
            }
        }
    }

    async fn state_update(&self, block_number: BlockNumber) -> ClientResult<BlockStateUpdate> {
        let mut url = self.urls.get_state_update.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.0.to_string());
        let raw_state_update = self.request_with_retry(url).await?;
        let state_update: BlockStateUpdate = serde_json::from_str(&raw_state_update)?;
        Ok(state_update)
    }
}
