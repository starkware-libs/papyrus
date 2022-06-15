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
    url: Url,
    internal_client: Client,
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

impl Display for StarknetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl StarknetClient {
    pub fn new(url_str: &str) -> Result<StarknetClient, ClientCreationError> {
        Ok(StarknetClient {
            url: Url::parse(url_str)?,
            internal_client: Client::builder().build()?,
        })
    }

    pub async fn block_number(&self) -> Result<BlockNumber, ClientError> {
        let block_number = self.request("feeder_gateway/get_last_batch_id").await?;
        Ok(BlockNumber(serde_json::from_str(&block_number)?))
    }

    pub async fn block_header(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockHeader, ClientError> {
        let query = format!("feeder_gateway/get_block?blockNumber={}", block_number.0);
        let raw_block = self.request(&query).await?;
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
        let query = format!(
            "feeder_gateway/get_state_update?blockNumber={}",
            block_number.0
        );
        let _raw_state_update: String = self.request(&query).await?;
        // TODO(dan): return the SN representation instead.
        let state_update: BlockStateUpdate = serde_json::from_str(&_raw_state_update)?;
        Ok(state_update)
    }

    async fn request(&self, path: &str) -> Result<String, ClientError> {
        // TODO(anatg): Move this to the constructor and set the query parameters here.
        let joined = self.url.join(path).unwrap();
        let response = self.internal_client.get(joined).send().await?;
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
