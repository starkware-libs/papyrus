pub mod objects;
#[cfg(test)]
mod serde_util_test;
mod serde_utils;
#[cfg(test)]
mod starknet_client_test;

use std::fmt::{self, Display, Formatter};

use log::{error, info};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::starknet::{BlockHeader, BlockNumber};

use self::objects::Block;

pub struct StarknetClient {
    url: url::Url,
    internal_client: reqwest::Client,
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
pub enum ClientError {
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

fn internal_client_error(err: impl Display) -> ClientError {
    panic!("Internal client error: {}.", err);
}

impl StarknetClient {
    pub fn new(url_str: &str) -> Result<StarknetClient, ClientError> {
        Ok(StarknetClient {
            url: Url::parse(url_str).map_err(internal_client_error)?,
            internal_client: Client::builder().build()?,
        })
    }

    pub async fn block_number(&self) -> Result<BlockNumber, ClientError> {
        let block_number = self.request("feeder_gateway/get_last_batch_id").await?;
        Ok(BlockNumber(serde_json::from_str(&block_number)?))
    }

    pub async fn block_header(&self, block_number: u32) -> Result<BlockHeader, ClientError> {
        let query = format!("feeder_gateway/get_block?blockNumber={}", block_number);
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

    async fn request(&self, path: &str) -> Result<String, ClientError> {
        let joined = self.url.join(path).map_err(internal_client_error)?;
        let response = self.internal_client.get(joined).send().await?;
        match response.status() {
            StatusCode::OK => {
                let body = response.text().await?;
                info!("Starknet client got response: {}.", body);
                Ok(body)
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.text().await?;
                let starknet_error: StarknetError = serde_json::from_str(&body)?;
                error!("Internal server error: {}.", starknet_error);
                Err(ClientError::StarknetError(starknet_error))
            }
            status if status.is_client_error() => {
                Err(internal_client_error(format!("{:?}", response)))
            }
            status if status.is_server_error() => {
                // This should not panic.
                let err = response.error_for_status().unwrap_err();
                error!("Server error: {}.", err);
                Err(ClientError::RequestError(err))
            }
            // TODO(anatg): Handle more errors, including reruns.
            _ => panic!("Unknown error: {:?}.", response),
        }
    }
}
