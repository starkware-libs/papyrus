use std::time::Duration;

use async_stream::stream;
use log::{debug, error, info};
use reqwest::StatusCode;
use tokio_stream::Stream;

use crate::starknet::{BlockHeader, BlockNumber};
use crate::starknet_client::{ClientCreationError, ClientError, StarknetClient};

pub struct CentralSource {
    starknet_client: StarknetClient,
}

impl CentralSource {
    pub fn new(url: &str) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(url)?;
        Ok(CentralSource { starknet_client })
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
                let res = self.starknet_client.block_header(current_block_number).await;
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
                            ClientError::BadResponse { status } => {
                                if status == StatusCode::TOO_MANY_REQUESTS {
                                    // TODO(dan): replace with a retry mechanism.
                                    debug!("Waiting for 5 sec.");
                                    tokio::time::sleep( Duration::from_millis(5000)).await;
                                } else {error!("{:?}",err); todo!()}
                            },
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
