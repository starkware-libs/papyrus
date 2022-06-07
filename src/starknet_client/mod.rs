#[cfg(test)]
mod object_test;
#[cfg(test)]
mod starknet_client_test;

mod objects;

use crate::starknet::BlockNumber;

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
}

impl StarknetClient {
    pub fn new(url_str: &str) -> Result<StarknetClient, ClientError> {
        Ok(StarknetClient {
            url: url::Url::parse(url_str)?,
            internal_client: reqwest::Client::builder().build()?,
        })
    }

    async fn request(&self, path: &str) -> Result<String, ClientError> {
        let joined = self.url.join(path)?;
        let res = self.internal_client.get(joined).send().await?;
        let body = res.text().await?;
        Ok(body)
    }

    pub async fn block_number(&self) -> Result<BlockNumber, ClientError> {
        let block_number = self.request("feeder_gateway/get_last_batch_id").await?;
        Ok(BlockNumber(block_number.parse()?))
    }
}
