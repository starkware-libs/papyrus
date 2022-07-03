use std::time::Duration;

use async_stream::stream;
use log::{debug, error, info};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use crate::starknet::{BlockHeader, BlockNumber};
use crate::starknet_client::{ClientCreationError, ClientError, StarknetClient};

#[derive(Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub url: String,
}
pub struct CentralSource {
    starknet_client: StarknetClient,
}

impl CentralSource {
    pub fn new(config: CentralSourceConfig) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(&config.url)?;
        info!("Central source is configured with {}.", config.url);
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
                let res = self.starknet_client.block(current_block_number).await;
                match res {
                    Ok(block) => {
                        info!("Received new block: {}.", block.block_number.0);
                        let header = BlockHeader {
                            block_hash: block.block_hash,
                            parent_hash: block.parent_block_hash,
                            number: block.block_number,
                            gas_price: block.gas_price,
                            state_root: block.state_root,
                            sequencer: block.sequencer_address,
                            timestamp: block.timestamp,
                            status: block.status.into(),
                        };
                        yield (current_block_number, header);
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
