use async_stream::stream;
use log::info;
use tokio_stream::Stream;

use crate::starknet::{BlockHeader, BlockNumber};
use crate::starknet_client::{ClientError, StarknetClient};

pub struct CentralSource {
    starknet_client: StarknetClient,
}

impl CentralSource {
    pub fn new(url: &str) -> Result<CentralSource, ClientError> {
        let starknet_client = StarknetClient::new(url)?;
        Ok(CentralSource { starknet_client })
    }

    pub async fn get_block_number(&mut self) -> Result<BlockNumber, ClientError> {
        self.starknet_client.block_number().await
    }

    pub fn stream_new_blocks(
        &mut self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> impl Stream<Item = Result<(BlockNumber, BlockHeader), ClientError>> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number <= up_to_block_number {
                let block_header = self.starknet_client.block_header(current_block_number).await?;
                info!("Received new header: {}.", block_header.number.0);
                yield Ok((current_block_number, block_header));
                current_block_number = current_block_number.next();
            }
        }
    }
}
