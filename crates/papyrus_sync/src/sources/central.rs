use std::sync::Arc;

use async_stream::stream;
use futures::{future, pin_mut, TryStreamExt};
use futures_util::StreamExt;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockBody, BlockHeader, BlockNumber, ClassHash, ContractClass, StateDiff};
use starknet_client::{
    client_to_starknet_api_storage_diff, BlockStateUpdate, ClientCreationError, ClientError,
    RetryConfig, StarknetClient, StarknetClientTrait,
};
use tokio_stream::Stream;

use super::stream_utils::MyStreamExt;

// TODO(dan): move to config.
const CONCURRENT_REQUESTS: usize = 300;

pub type CentralResult<T> = Result<T, CentralError>;
#[derive(Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub url: String,
    pub retry_config: RetryConfig,
}
pub struct GenericCentralSource<TStarknetClient: StarknetClientTrait + Send + Sync> {
    pub starknet_client: Arc<TStarknetClient>,
}

#[derive(thiserror::Error, Debug)]
pub enum CentralError {
    #[error(transparent)]
    ClientError(#[from] Arc<ClientError>),
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
    ) -> impl Stream<Item = CentralResult<(BlockNumber, StateDiff)>> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number < up_to_block_number {
                let state_update_stream = self
                    .state_update_stream(
                        futures_util::stream::iter(current_block_number.0..up_to_block_number.0)
                            .map(|block_number| BlockNumber(block_number)),
                    );
                pin_mut!(state_update_stream);
                while let Some(maybe_state_update) = state_update_stream.next().await{
                    let (state_update, classes) = match maybe_state_update{
                        Ok(((state_update, classes))) => (state_update, classes),
                        Err(err) => {
                          match err{
                              _ => yield (Err(err)),
                          }
                          return;
                        }
                    };
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
                let mut res =
                    futures_util::stream::iter(current_block_number.0..up_to_block_number.0)
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
                            debug!(
                                "Received error for block {}: {:?}.",
                                current_block_number.0, err
                            );
                            yield (Err(CentralError::ClientError(Arc::new(err))));
                            return;
                        }
                    }
                }
            }
        }
    }

    fn state_update_stream(
        &self,
        block_number_stream: impl Stream<Item = BlockNumber> + Send + Sync + 'static,
    ) -> impl Stream<Item = CentralResult<(BlockStateUpdate, Vec<(ClassHash, ContractClass)>)>>
    {
        let starknet_client = self.starknet_client.clone();
        let (state_updates0, mut state_updates1) = block_number_stream
            .map(move |block_number| {
                let starknet_client = starknet_client.clone();
                async move { starknet_client.state_update(block_number).await }
            })
            .buffered(CONCURRENT_REQUESTS)
            // Client error is not cloneable.
            .map_err(Arc::new)
            .fanout(CONCURRENT_REQUESTS);
        let starknet_client = self.starknet_client.clone();
        let mut flat_classes = state_updates0
            // In case state_updates1 contains a ClientError, we yield it and break - without
            // evaluating flat_classes.
            .filter_map(|s| future::ready(s.ok()))
            .map(|state_update| state_update.state_diff.class_hashes())
            .flat_map(futures::stream::iter)
            .map(move |class_hash| {
                let starknet_client = starknet_client.clone();
                async move { (class_hash, starknet_client.class_by_hash(class_hash).await) }
            })
            .buffered(CONCURRENT_REQUESTS)
            .map(|(class_hash, class)| (class_hash, class.map_err(Arc::new)));
        let res_stream = stream! {
            while let Some(maybe_state_update) = state_updates1.next().await {
                let state_update = match maybe_state_update {
                    Ok(state_update) => state_update,
                    Err(err) => {
                        match err {
                            _ => yield (Err(CentralError::ClientError(err))),
                        }
                        break;
                    }
                };
                let len = state_update.state_diff.class_hashes().len();
                let classes: Result<Vec<(ClassHash, ContractClass)>, _> = flat_classes
                    .take_n(len)
                    .await
                    .expect("Failed to download state update")
                    .into_iter()
                    .map(|(class_hash, class)| {
                        if class.is_ok() {
                            Ok((class_hash, class.ok().unwrap()))
                        } else {
                            Err(CentralError::ClientError(class.err().unwrap()))
                        }
                    })
                    .collect();
                match classes {
                    Ok(classes) => yield (Ok((state_update, classes))),
                    Err(err) => yield (Err(err)),
                }
            }
        };
        res_stream
    }
}

pub type CentralSource = GenericCentralSource<StarknetClient>;

impl CentralSource {
    pub fn new(config: CentralSourceConfig) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(&config.url, config.retry_config)?;
        info!("Central source is configured with {}.", config.url);
        Ok(CentralSource { starknet_client: Arc::new(starknet_client) })
    }
}
