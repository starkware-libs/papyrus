//! This module contains clients that can request changes to [`Starknet`].
//!
//! [`Starknet`]: https://starknet.io/

pub mod objects;

#[cfg(test)]
mod starknet_gateway_client_test;

use std::collections::HashMap;

use async_trait::async_trait;
use url::Url;

use self::objects::response::AddTransactionResponse;
use self::objects::transaction::Transaction;
use crate::{ClientCreationError, ClientError, ClientResult, RetryConfig, StarknetBaseClient};

/// A trait describing an object that can communicate with Starknet and make changes to it.
///
/// [`Starknet`]: https://starknet.io/
#[async_trait]
pub trait StarknetWriter {
    /// Add a transaction to [`Starknet`].
    ///
    /// [`Starknet`]: https://starknet.io/
    async fn add_transaction(&self, tx: Transaction) -> ClientResult<AddTransactionResponse>;
}

const ADD_TRANSACTION_URL_SUFFIX: &str = "gateway/add_transaction";

/// A client for the [`Starknet`] gateway.
///
/// [`Starknet`]: https://starknet.io/
struct StarknetGatewayClient {
    add_transaction_url: Url,
    client: StarknetBaseClient,
}

#[async_trait]
impl StarknetWriter for StarknetGatewayClient {
    async fn add_transaction(
        &self,
        tx: Transaction,
    ) -> Result<AddTransactionResponse, ClientError> {
        let response: String = self
            .client
            .request_with_retry(
                self.client
                    .internal_client
                    .post(self.add_transaction_url.clone())
                    .body(serde_json::to_string(&tx)?),
            )
            .await?;
        Ok(serde_json::from_str::<AddTransactionResponse>(&response)?)
    }
}

impl StarknetGatewayClient {
    pub fn new(
        starknet_url: &str,
        http_headers: Option<HashMap<String, String>>,
        node_version: &'static str,
        retry_config: RetryConfig,
    ) -> Result<Self, ClientCreationError> {
        Ok(StarknetGatewayClient {
            add_transaction_url: Url::parse(starknet_url)?.join(ADD_TRANSACTION_URL_SUFFIX)?,
            client: StarknetBaseClient::new(http_headers, node_version, retry_config)?,
        })
    }
}
