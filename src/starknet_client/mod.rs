pub mod objects;
#[cfg(test)]
mod serde_util_test;
mod serde_utils;
#[cfg(test)]
mod starknet_client_test;

use crate::starknet::{BlockHeader, BlockNumber};

use self::objects::Block;

pub struct StarknetClient {
    url: url::Url,
    internal_client: reqwest::Client,
}
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    BadUrl(#[from] url::ParseError),
    #[error(transparent)]
    BadRequest(#[from] reqwest::Error),
    #[error(transparent)]
    BadResponse(#[from] core::num::ParseIntError),
    #[error(transparent)]
    BadResponseJson(#[from] serde_json::Error),
}

impl StarknetClient {
    pub fn new(url_str: &str) -> anyhow::Result<StarknetClient, ClientError> {
        Ok(StarknetClient {
            url: url::Url::parse(url_str)?,
            internal_client: reqwest::Client::builder().build()?,
        })
    }

    pub async fn block_number(&self) -> Result<BlockNumber, ClientError> {
        let block_number = self.request("feeder_gateway/get_last_batch_id").await?;
        Ok(BlockNumber(block_number.parse()?))
    }

    pub async fn block_header(&self, block_number: u32) -> Result<BlockHeader, ClientError> {
        let mut query: String = "feeder_gateway/get_block?blockNumber=".to_owned();
        query.push_str(&block_number.to_string());
        let _raw_block: String = self.request(&query).await?;
        let block: Block = serde_json::from_str(&_raw_block)?;
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
        let joined = self.url.join(path)?;
        let res = self.internal_client.get(joined).send().await?;
        let body = res.text().await?;
        Ok(body)
    }
}
