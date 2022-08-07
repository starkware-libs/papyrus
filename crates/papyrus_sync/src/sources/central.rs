use std::sync::Arc;

use async_stream::stream;
use futures::{pin_mut, FutureExt};
use futures_util::StreamExt;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockBody, BlockHeader, BlockNumber, ClassHash, ContractClass, StateDiff};
use starknet_client::{
    client_to_starknet_api_storage_diff, BlockStateUpdate, ClientCreationError, ClientError,
    StarknetClient, StarknetClientTrait,
};
use tokio_stream::Stream;

use super::stream_utils::MyStreamExt;

// TODO(dan): move to config.
const CONCURRENT_REQUESTS: usize = 200;

#[derive(Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub url: String,
}
pub struct GenericCentralSource<TStarknetClient: StarknetClientTrait + Send + Sync> {
    pub starknet_client: Arc<TStarknetClient>,
}

#[derive(thiserror::Error, Debug)]
pub enum CentralError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
}

impl<TStarknetClient: StarknetClientTrait + Send + Sync + 'static>
    GenericCentralSource<TStarknetClient>
{
    pub async fn get_block_marker(&self) -> Result<BlockNumber, ClientError> {
        self.starknet_client
            .block_number()
            .await?
            .map_or(Ok(BlockNumber::default()), |block_number| Ok(block_number.next()))
    }

    pub fn stream_state_updates(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> impl Stream<Item = Result<(BlockNumber, StateDiff), CentralError>> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number < up_to_block_number {
                let state_update_stream = self
                    .state_update_stream(
                        futures_util::stream::iter(current_block_number.0..up_to_block_number.0)
                            .map(|block_number| BlockNumber(block_number)),
                    )
                    .fuse()
                    .await;
                pin_mut!(state_update_stream);
                while let Some((state_update, classes)) = state_update_stream.next().await{
                    let state_diff_forward = StateDiff {
                        deployed_contracts: state_update.state_diff.deployed_contracts,
                        storage_diffs: client_to_starknet_api_storage_diff(
                            state_update.state_diff.storage_diffs),
                        declared_classes: classes,
                        // TODO(dan): fix once nonces are available.
                        nonces: vec![],
                    };
                    yield Ok((current_block_number, state_diff_forward));
                    current_block_number = current_block_number.next();
                }
            }
        }
    }

    // TODO(dan): return all block data.
    pub fn stream_new_blocks(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> impl Stream<Item = Result<(BlockNumber, BlockHeader, BlockBody), CentralError>> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number < up_to_block_number {
                let mut res = futures_util::stream::iter(current_block_number.0..up_to_block_number.0)
                    .map(|bn| async move { self.starknet_client.block(BlockNumber(bn)).await })
                    .buffered(CONCURRENT_REQUESTS);
                while let Some(maybe_block) = res.next().await {
                    match maybe_block {
                        Ok(Some(block)) => {
                            info!("Received new block: {}.", block.block_number.0);
                            let header = BlockHeader {
                                block_hash: block.block_hash,
                                parent_hash: block.parent_block_hash,
                                block_number: block.block_number,
                                gas_price: block.gas_price,
                                state_root: block.state_root,
                                sequencer: block.sequencer_address,
                                timestamp: block.timestamp,
                                status: block.status.into(),
                            };
                            let body = BlockBody {
                                transactions: block
                                    .transactions
                                    .into_iter()
                                    .map(|x| x.into())
                                    .collect(),
                            };
                            yield Ok((current_block_number, header, body));
                            current_block_number = current_block_number.next();
                        }
                        Ok(None) => todo!(),
                        Err(err) => {
                            debug!("Received error for block {}: {:?}.", current_block_number.0, err);
                            yield (Err(CentralError::ClientError(err)))
                        }
                    }
                }
            }
        }
    }

    async fn state_update_stream(
        &self,
        input: impl Stream<Item = BlockNumber> + Send + Sync + 'static,
    ) -> impl Stream<Item = (BlockStateUpdate, Vec<(ClassHash, ContractClass)>)> {
        let starknet_client = self.starknet_client.clone();
        let (receiver_0, mut receiver_1) = input
            .map(move |block_number| {
                let starknet_client = starknet_client.clone();
                async move { starknet_client.state_update(block_number).await }
            })
            .buffered(CONCURRENT_REQUESTS)
            // TODO(dan): fix this.
            .map(|a| a.unwrap())
            .fanout(CONCURRENT_REQUESTS);
        let starknet_client = self.starknet_client.clone();
        let mut flat_classes = receiver_0
            .map(|state_update| state_update.state_diff.class_hashes())
            .flat_map(futures::stream::iter)
            .map(move |class_hash| {
                let starknet_client = starknet_client.clone();
                async move { (class_hash, starknet_client.class_by_hash(class_hash).await) }
            })
            .buffered(CONCURRENT_REQUESTS)
            .map(|(class_hash, class)| (class_hash, class.unwrap()));
        let res_stream = stream! {
            while let Some(state_update) = receiver_1.next().await{
                let len = state_update.state_diff.class_hashes().len();
                // TODO(dan): fix this.
                let classes = flat_classes.take_n(len).await.unwrap();
                yield (state_update, classes);
            }
        };
        res_stream
    }
}

pub type CentralSource = GenericCentralSource<StarknetClient>;

impl CentralSource {
    pub fn new(config: CentralSourceConfig) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(&config.url)?;
        info!("Central source is configured with {}.", config.url);
        Ok(CentralSource { starknet_client: Arc::new(starknet_client) })
    }
}
