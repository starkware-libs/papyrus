#[cfg(test)]
mod starknet_client_test;

use serde::{Deserialize, Serialize};

use crate::starknet::{
    BlockHash, BlockHeader, BlockNumber, BlockTimestamp, ContractAddress, GasPrice, GlobalRoot,
    Status,
};

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

#[derive(Debug, Deserialize, Serialize)]
struct Block {
    block_hash: BlockHash,
    block_number: BlockNumber,
    gas_price: GasPrice,
    parent_block_hash: BlockHash,
    sequencer_address: ContractAddress,
    state_root: GlobalRoot,
    status: Status,
    timestamp: BlockTimestamp,
    // TODO(dan): define corresponding structs and handle properly.
    transaction_receipts: Vec<serde_json::Value>,
    transactions: Vec<serde_json::Value>,
}

#[allow(dead_code)]
impl StarknetClient {
    pub fn new(url_str: &str) -> anyhow::Result<StarknetClient, ClientError> {
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

    pub async fn block_header(&self, block_number: u32) -> Result<BlockHeader, ClientError> {
        let mut query: String = "feeder_gateway/get_block?blockNumber=".to_owned();
        query.push_str(&block_number.to_string());
        let _raw_block: String = self.request(&query).await?;
        let block: Block = serde_json::from_str(&_raw_block)?;
        Ok(BlockHeader {
            block_hash: block.block_hash,
            parent_hash: block.parent_block_hash,
            number: block.block_number,
            gas_price: block.gas_price,
            state_root: block.state_root,
            sequencer: block.sequencer_address,
            timestamp: block.timestamp,
        })
    }
}
