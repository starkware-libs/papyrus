use std::sync::Arc;

use async_stream::stream;
use futures::{future, pin_mut, TryStreamExt};
use futures_util::StreamExt;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{Block, BlockNumber, DeclaredContract, StarknetApiError, StateDiff};
use starknet_client::{
    client_to_starknet_api_storage_diff, ClientCreationError, ClientError, RetryConfig,
    StarknetClient, StarknetClientTrait, StateUpdate,
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
    #[error("Could not find a state update.")]
    StateUpdateNotFound,
    #[error("Could not find a block with block number {:?}.", block_number)]
    BlockNotFound { block_number: BlockNumber },
    #[error(transparent)]
    StarknetApiError(#[from] Arc<StarknetApiError>),
}

fn get_state_diff(
    maybe_state_update: CentralResult<(StateUpdate, Vec<DeclaredContract>)>,
) -> CentralResult<StateDiff> {
    let (state_update, classes) = maybe_state_update?;
    Ok(StateDiff::new(
        state_update.state_diff.deployed_contracts,
        client_to_starknet_api_storage_diff(state_update.state_diff.storage_diffs),
        classes,
        // TODO(dan): fix once nonces are available.
        vec![],
    ))
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
                        futures_util::stream::iter(current_block_number.iter_up_to(up_to_block_number))
                    );
                pin_mut!(state_update_stream);
                while let Some(maybe_state_update) = state_update_stream.next().await{
                    let state_diff = get_state_diff(maybe_state_update);
                    match state_diff {
                        Ok(state_diff) => {
                            yield Ok((current_block_number, state_diff));
                            current_block_number = current_block_number.next();
                        }
                        Err(err) => {
                            debug!("Block number {}: {:#?}", current_block_number.str(), err);
                            yield Err(err);
                            return;
                        }
                    }
                }
            }
        }
    }

    // TODO(shahak): rename.
    pub fn stream_new_blocks(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> impl Stream<Item = Result<(BlockNumber, Block), CentralError>> + '_ {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number < up_to_block_number {
                let mut res =
                    futures_util::stream::iter(current_block_number.iter_up_to(up_to_block_number))
                        .map(|bn| async move { self.starknet_client.block(bn).await })
                        .buffered(CONCURRENT_REQUESTS);
                while let Some(maybe_block) = res.next().await {
                    let res = match maybe_block {
                        Ok(Some(block)) => {
                            info!("Received new block: {}.", block.block_number.str());
                            Block::try_from(block)
                                .map_err(|err| CentralError::ClientError(Arc::new(err)))
                        }
                        Ok(None) => {
                            Err(CentralError::BlockNotFound { block_number: current_block_number })
                        }
                        Err(err) => Err(CentralError::ClientError(Arc::new(err))),
                    };
                    match res {
                        Ok(block) => {
                            yield Ok((current_block_number, block));
                            current_block_number = current_block_number.next();
                        }
                        Err(err) => {
                            debug!(
                                "Received error for block {}: {:?}.",
                                current_block_number.str(), err
                            );
                            yield (Err(err));
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
    ) -> impl Stream<Item = CentralResult<(StateUpdate, Vec<DeclaredContract>)>> {
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
            .filter_map(|state_update| future::ready(state_update.ok()))
            .filter_map(future::ready)
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
                    Ok(Some(state_update)) => state_update,
                    Ok(None) => {
                        yield (Err(CentralError::StateUpdateNotFound));
                        break;
                    }
                    Err(err) => {
                        match err {
                            _ => yield (Err(CentralError::ClientError(err))),
                        }
                        break;
                    }
                };
                let len = state_update.state_diff.class_hashes().len();
                let classes: Result<Vec<DeclaredContract>, _> = flat_classes
                    .take_n(len)
                    .await
                    .expect("Failed to download state update")
                    .into_iter()
                    .map(|(class_hash, class)| {
                        match class{
                            Ok(Some(class)) => Ok(DeclaredContract { class_hash, contract_class: class }),
                            Ok(None) => Err(CentralError::StateUpdateNotFound),
                            Err(err) => Err(CentralError::ClientError(err)),
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
