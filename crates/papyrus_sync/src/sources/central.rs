use std::collections::HashSet;
use std::sync::Arc;

use async_stream::stream;
use futures_util::StreamExt;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockBody, BlockHeader, BlockNumber, ClassHash, ContractClass, StateDiff};
use starknet_client::{
    client_to_starknet_api_storage_diff, BlockStateUpdate, ClientCreationError, ClientError,
    ClientResult, StarknetClient, StarknetClientTrait,
};
use tokio_stream::Stream;

use super::stream_utils::MyStreamExt;

// TODO(dan): move to config.
const CONCURRENT_REQUESTS: usize = 750;

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

impl<TStarknetClient: StarknetClientTrait + Send + Sync> GenericCentralSource<TStarknetClient> {
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
                let mut state_update_stream =
                    futures_util::stream::iter(current_block_number.0..up_to_block_number.0)
                        .map(|block_number| async move {
                            self.starknet_client.state_update(BlockNumber(block_number)).await
                        })
                        .buffered(CONCURRENT_REQUESTS);
                while let Some(maybe_state_update) = state_update_stream.next().await {
                    let state_update = match maybe_state_update{
                        Ok(state_update) => state_update,
                        Err(err) => {
                          debug!("Received error for state diff {}: {:?}.", current_block_number.0, err);
                          // TODO(dan): proper error handling.
                          match err{
                              _ => yield (Err(CentralError::ClientError(err))),
                          }
                          continue;
                        }
                    };

                    let class_hashes = state_update.state_diff.class_hashes();
                    let classes = self.fetch_contract_classes(class_hashes).await;
                    if classes.is_err(){
                        yield (Err(CentralError::ClientError(classes.err().unwrap())));
                        break;
                    }
                    debug!("Received new state update: {:?}.", current_block_number.0);
                    let state_diff_forward = StateDiff {
                        deployed_contracts: state_update.state_diff.deployed_contracts,
                        storage_diffs: client_to_starknet_api_storage_diff(
                            state_update.state_diff.storage_diffs),
                        declared_classes: classes.unwrap(),
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

    async fn fetch_contract_classes(
        &self,
        class_hashes: HashSet<ClassHash>,
    ) -> ClientResult<Vec<(ClassHash, ContractClass)>> {
        let classes_stream = futures_util::stream::iter(class_hashes)
            .map(|class_hash| async move {
                (class_hash, self.starknet_client.class_by_hash(class_hash).await)
            })
            .buffer_unordered(CONCURRENT_REQUESTS);
        let maybe_classes: Vec<_> = classes_stream.collect().await;
        let mut classes = Vec::new();
        for (class_hash, maybe_class) in maybe_classes {
            classes.push((class_hash, maybe_class?));
        }
        Ok(classes)
    }

    async fn my_stream(
        &self,
        input: impl Stream<Item = BlockNumber> + Send + Sync + 'static,
    ) -> impl Stream<Item = (BlockStateUpdate, Vec<(ClassHash, ContractClass)>)> {
        // ) -> Result<impl Stream<Item = (BlockStateUpdate, Vec<usize>)>, ClientError> {
        // ) -> impl Stream<Item = (ClientResult<BlockStateUpdate>, Vec<usize>)> {
        let starknet_client = self.starknet_client.clone();
        let (ns0, mut ns1) = input
            .map(move |block_number| {
                let starknet_client:Arc<TStarknetClient>= starknet_client.clone();
                async move {
                starknet_client.state_update(BlockNumber(1)).await;
                BlockStateUpdate::default()
            }
        })
            .buffered(CONCURRENT_REQUESTS)
            // .map(|a| a.unwrap())
            .fanout(CONCURRENT_REQUESTS);
        let mut flat_classes = ns0
            .map(|state_update| state_update.state_diff.class_hashes())
            .flat_map(|class_hashes| futures::stream::iter(class_hashes))
            .map(|class_hash| async move {
                (class_hash, starknet_client.class_by_hash(class_hash).await)
            })
            .buffered(CONCURRENT_REQUESTS)
            .map(|(class_hash, class)| (class_hash, class.unwrap()));
        let s = stream! {
            while let Some(state_update) = ns1.next().await{
                let len = state_update.state_diff.class_hashes().len();
                let res = flat_classes.take_n(len).await.unwrap();
                yield (state_update, res);
            }
        };
        s
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
