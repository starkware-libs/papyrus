use std::collections::HashSet;
use std::time::Duration;

use async_stream::stream;
use papyrus_p2p::client::Client;
use papyrus_p2p::{BlockHeader, BlockNumber, PeerId};
use tokio_stream::Stream;

pub struct P2PSource {
    pub network_client: Client,
    pub peers: HashSet<PeerId>,
}

pub type P2PSourceResult<T> = Result<T, P2PSourceError>;

#[derive(thiserror::Error, Debug)]
pub enum P2PSourceError {
    #[error("Could not find a block with block number {:?}.", block_number)]
    BlockNotFound { block_number: BlockNumber },
}

impl P2PSource {
    pub fn new(network_client: Client) -> Self {
        P2PSource { network_client, peers: Default::default() }
    }

    pub async fn get_block_marker(&self) -> BlockNumber {
        log::error!("get_block_marker, returning BlockNumber::new(100)");
        BlockNumber::new(100)
    }

    pub fn stream_new_blocks(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> impl Stream<Item = P2PSourceResult<(BlockNumber, BlockHeader)>> + '_ {
        log::error!("stream_new_blocks");
        let mut network_client = self.network_client.clone();
        log::error!("{network_client:?}");
        let mut current_block_number = initial_block_number;
        stream! {
            let mut peers = network_client.get_peers().await;
            while peers.is_empty(){
                tokio::time::sleep(Duration::from_secs(10)).await;
                peers = network_client.get_peers().await;
                log::error!("{peers:?}");
            }
            log::error!("--------------------- {peers:?}");

            let peer = peers.iter().next().unwrap();
            log::error!("{peer:?}");
            while current_block_number < up_to_block_number {
                log::info!(" up_to_block_number {up_to_block_number}");
                log::info!(" pre - current_block_number {current_block_number}");
                let block_header = network_client.get_block_headers(*peer,current_block_number).await.unwrap();
                log::info!(" block_header {block_header:?}");
                yield Ok((current_block_number, block_header));
                log::info!(" post - current_block_number {current_block_number}");
                current_block_number = current_block_number.next();
            }
        }
    }
}
