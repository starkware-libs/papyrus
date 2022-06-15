mod objects;
#[cfg(test)]
mod serde_util_test;
mod serde_utils;
#[cfg(test)]
mod starknet_client_test;

use std::fmt::{self, Display, Formatter};

use log::{debug, error, info};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::starknet::{BlockHeader, BlockNumber};

use self::objects::block::{Block, BlockStateUpdate};

pub struct StarknetClient {
    urls: StarknetUrls,
    internal_client: Client,
}
#[derive(Clone, Debug)]
struct StarknetUrls {
    get_last_batch_id: Url,
    get_block: Url,
    get_state_update: Url,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
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

impl StarknetUrls {
    fn new(url_str: &str) -> Result<Self, ClientCreationError> {
        let base_url = Url::parse(url_str)?;
        Ok(StarknetUrls {
            get_last_batch_id: base_url.join("feeder_gateway/get_last_batch_id")?,
            get_block: base_url.join("feeder_gateway/get_block")?,
            get_state_update: base_url.join("feeder_gateway/get_state_update")?,
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

    pub async fn block_number(&self) -> Result<BlockNumber, ClientError> {
        let block_number = self.request(self.urls.get_last_batch_id.clone()).await?;
        Ok(BlockNumber(serde_json::from_str(&block_number)?))
    }

    pub async fn block_header(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockHeader, ClientError> {
        let mut url = self.urls.get_block.clone();
        url.query_pairs_mut()
            .append_pair("blockNumber", &block_number.0.to_string());
        let raw_block = self.request(url).await?;
        let block: Block = serde_json::from_str(&raw_block)?;
        Ok(BlockHeader {
            block_hash: block.block_hash.into(),
            parent_hash: block.parent_block_hash.into(),
            number: block.block_number.into(),
            gas_price: block.gas_price.into(),
            state_root: block.state_root.into(),
            sequencer: block.sequencer_address.into(),
            timestamp: block.timestamp.into(),
        })
    }

    pub async fn state_update(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockStateUpdate, ClientError> {
        let mut url = self.urls.get_state_update.clone();
        url.query_pairs_mut()
            .append_pair("blockNumber", &block_number.0.to_string());
        let raw_state_update = self.request(url).await?;
        // TODO(dan): return the SN representation instead.
        let state_update: BlockStateUpdate = serde_json::from_str(&raw_state_update)?;
        Ok(state_update)
    }
    async fn request(&self, url: Url) -> Result<String, ClientError> {
        let response = self.internal_client.get(url).send().await?;
        match response.status() {
            StatusCode::OK => {
                let body = response.text().await?;
                debug!("Starknet server responded with: {}.", body);
                Ok(body)
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.text().await?;
                let starknet_error: StarknetError = serde_json::from_str(&body)?;
                info!(
                    "Starknet server responded with an internal server error: {}.",
                    starknet_error
                );
                Err(ClientError::StarknetError(starknet_error))
            }
            _ => {
                error!("Bad response: {:?}.", response);
                Err(ClientError::BadResponse {
                    status: response.status(),
                })
            }
        }
    }
}
