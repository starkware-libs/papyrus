use std::time::Duration;

use async_stream::stream;
use log::info;
use tokio_stream::Stream;

use crate::starknet::{BlockHeader, BlockNumber};
use crate::starknet_client::{ClientError, StarknetClient};

pub struct CentralSource {
    starknet_client: StarknetClient,
}

// TODO(spapini): Take from config.
const SLEEP_DURATION: Duration = Duration::from_millis(10000);

impl CentralSource {
    pub fn new(url: &str) -> Result<CentralSource, ClientError> {
        let starknet_client = StarknetClient::new(url)?;
        Ok(CentralSource { starknet_client })
    }

    pub async fn get_block_number(&mut self) -> Result<BlockNumber, ClientError> {
        self.starknet_client.block_number().await
    }

    // TODO(spapini): Return blocks instead of numbers.
    pub fn stream_new_blocks(
        &mut self,
        initial_block_number: BlockNumber,
        up_to_block_number: Option<BlockNumber>,
    ) -> impl Stream<Item = Result<(BlockNumber, BlockHeader), ClientError>> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            loop{
                // TODO(dan): figure out how to unwarp_or_else async.
                let latest_block_number = up_to_block_number.unwrap_or(self.get_block_number().await?);
                while current_block_number <= latest_block_number {
                    let block_header = self.starknet_client.block_header(current_block_number.0).await?;
                    info!("Received new header: {}.", block_header.number.0);
                    yield Ok((current_block_number, block_header));
                    current_block_number = current_block_number.next();
                }
                if up_to_block_number.is_some() {
                    break;
                }
                tokio::time::sleep(SLEEP_DURATION).await
            }
        }
    }
}
