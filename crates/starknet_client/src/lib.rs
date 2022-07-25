mod objects;
#[cfg(test)]
mod starknet_client_test;
#[cfg(test)]
mod test_utils;

use std::fmt::{self, Display, Formatter};

use log::{error, info};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockNumber, ClassHash, ContractClass};
use url::Url;

pub use self::objects::block::{Block, BlockStateUpdate};

pub struct StarknetClient {
    urls: StarknetUrls,
    internal_client: Client,
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

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("Response status code: {:?}", status)]
    BadResponse { status: reqwest::StatusCode },
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
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
    pub fn new(url_str: &str) -> Result<StarknetClient, ClientCreationError> {
        Ok(StarknetClient {
            urls: StarknetUrls::new(url_str)?,
            internal_client: Client::builder().build()?,
        })
    }

    pub async fn block_number(&self) -> Result<Option<BlockNumber>, ClientError> {
        let response = self.request(self.urls.get_block.clone()).await;
        match response {
            Ok(raw_block) => {
                let block: Block = serde_json::from_str(&raw_block)?;
                Ok(Some(block.block_number))
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

    pub async fn block(&self, block_number: BlockNumber) -> Result<Option<Block>, ClientError> {
        let mut url = self.urls.get_block.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.0.to_string());
        let response = self.request(url).await;
        match response {
            Ok(raw_block) => {
                let block: Block = serde_json::from_str(&raw_block)?;
                Ok(Some(block))
            }
            Err(err) => match err {
                ClientError::StarknetError(sn_err) => {
                    let StarknetError { code, message } = sn_err;
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

    pub async fn class_by_hash(&self, class_hash: ClassHash) -> Result<ContractClass, ClientError> {
        let mut url = self.urls.get_contract_by_hash.clone();
        let class_hash = serde_json::to_string(&class_hash)?;
        url.query_pairs_mut()
            .append_pair(CLASS_HASH_QUERY, &class_hash.as_str()[1..class_hash.len() - 1]);
        let response = self.request(url).await;
        match response {
            Ok(raw_contract_class) => Ok(serde_json::from_str(&raw_contract_class)?),
            Err(err) => {
                error!("{}", err);
                Err(err)
            }
        }
    }

    pub async fn state_update(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockStateUpdate, ClientError> {
        let mut url = self.urls.get_state_update.clone();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.0.to_string());
        let raw_state_update = self.request(url).await?;
        let state_update: BlockStateUpdate = serde_json::from_str(&raw_state_update)?;
        Ok(state_update)
    }

    async fn request(&self, url: Url) -> Result<String, ClientError> {
        let response = self.internal_client.get(url).send().await?;
        match response.status() {
            StatusCode::OK => {
                let body = response.text().await?;
                Ok(body)
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.text().await?;
                let starknet_error: StarknetError = serde_json::from_str(&body)?;
                // TODO(dan): consider logging as error instead.
                info!(
                    "Starknet server responded with an internal server error: {}.",
                    starknet_error
                );
                Err(ClientError::StarknetError(starknet_error))
            }
            _ => {
                // TODO(dan): consider logging as info instead.
                error!("Bad response: {:?}.", response);
                Err(ClientError::BadResponse { status: response.status() })
            }
        }
    }
}
