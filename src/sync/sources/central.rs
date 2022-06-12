#[cfg(test)]
#[path = "central_test.rs"]
mod central_test;

use std::iter::Take;
use std::time::Duration;

use async_stream::stream;
use log::{debug, error, info};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_retry::{Action, RetryIf};
use tokio_stream::Stream;

use crate::starknet::{BlockHeader, BlockNumber};
use crate::starknet_client::{ClientCreationError, ClientError, StarknetClient};

#[derive(Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub url: String,
    pub retry_base_millis: u64,
    pub retry_max_delay_millis: u64,
    pub max_retries: usize,
}

pub struct CentralSource {
    starknet_client: StarknetClient,
    retry_strategy: Take<ExponentialBackoff>,
}

impl CentralSource {
    pub fn new(config: CentralSourceConfig) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(&config.url)?;
        let retry_strategy = ExponentialBackoff::from_millis(config.retry_base_millis)
            .max_delay(Duration::from_millis(config.retry_max_delay_millis))
            .take(config.max_retries);
        Ok(CentralSource {
            starknet_client,
            retry_strategy,
        })
    }

    fn should_retry(err: &ClientError) -> bool {
        match err {
            ClientError::BadResponse { status } if *status == StatusCode::TOO_MANY_REQUESTS => {
                debug!("Received error {:?}, retrying.", err);
                true
            }
            _ => {
                debug!("Received error {:?}, not retrying.", err);
                false
            }
        }
    }

    async fn retry<I, T>(&self, action: T) -> Result<I, ClientError>
    where
        T: Action<Item = I, Error = ClientError>,
    {
        RetryIf::spawn(
            self.retry_strategy.clone(),
            action,
            CentralSource::should_retry,
        )
        .await
    }

    pub async fn get_block_number(&mut self) -> Result<BlockNumber, ClientError> {
        self.starknet_client.block_number().await
    }

    // TODO(dan): return all block data.
    pub fn stream_new_blocks(
        &mut self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> impl Stream<Item = (BlockNumber, BlockHeader)> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number <= up_to_block_number {
                let res = self.retry(|| self.starknet_client.block_header(current_block_number)).await;
                match res {
                    Ok(block_header) => {
                        info!("Received new header: {}.", block_header.number.0);
                        yield (current_block_number, block_header);
                        current_block_number = current_block_number.next();
                    },
                    Err(err) => {
                        debug!("Received error for block {}: {:?}.", current_block_number.0, err);
                        // TODO(dan): proper error handling.
                        match err{
                            ClientError::BadResponse { status: _ } => {error!("{:?}",err); todo!()},
                            ClientError::RequestError(err) => {error!("{:?}",err); todo!()},
                            ClientError::SerdeError(err) => {error!("{:?}",err); todo!()},
                            ClientError::StarknetError(err) => {error!("{:?}",err); todo!()},

                        }
                    }
                }
            }
        }
    }
}
