use async_stream::stream;
use futures_util::StreamExt;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockBody, BlockHeader, BlockNumber, ClassHash, ContractClass, StateDiff};
use starknet_client::{
    client_to_starknet_api_storage_diff, ClientCreationError, ClientError, StarknetClient,
};
use tokio_stream::Stream;

// TODO(dan): move to config.
const CONCURRENT_REQUESTS: usize = 750;

#[derive(Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub url: String,
}
pub struct CentralSource {
    starknet_client: StarknetClient,
}

#[derive(thiserror::Error, Debug)]
pub enum CentralError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
}

impl CentralSource {
    pub fn new(config: CentralSourceConfig) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(&config.url)?;
        info!("Central source is configured with {}.", config.url);
        Ok(CentralSource { starknet_client })
    }

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
                let mut res = futures_util::stream::iter(current_block_number.0..up_to_block_number.0)
                    .map(|bn| async move { self.starknet_client.state_update(BlockNumber(bn)).await })
                    .buffered(CONCURRENT_REQUESTS);
                while let Some(maybe_state_update) = res.next().await {
                    match maybe_state_update {
                        Ok(state_update) => {
                            let class_hashes = state_update.state_diff.all_class_hashes();
                            let res = futures_util::stream::iter(class_hashes)
                                .map(|class_hash| async move { (class_hash, self.starknet_client.class_by_hash(class_hash).await) })
                                .buffer_unordered(CONCURRENT_REQUESTS);
                            let maybe_classes: Vec<(ClassHash, Result<ContractClass, ClientError>)>= res.collect().await;
                            let (classes, mut errors): (Vec<_>, Vec<_>) = maybe_classes.into_iter().partition(|x| x.1.is_ok());
                            if !errors.is_empty(){
                                let client_error =errors.pop().unwrap().1.err().unwrap();
                                yield (Err(CentralError::ClientError(client_error)));
                                break;
                            }
                            debug!("Received new state update: {:?}.", current_block_number.0);
                            let classes: Vec<(ClassHash, ContractClass)> = classes.into_iter().map(|x| (x.0, x.1.ok().unwrap())).collect();
                            let state_diff_forward = StateDiff {
                                deployed_contracts: state_update.state_diff.deployed_contracts,
                                storage_diffs: client_to_starknet_api_storage_diff(state_update.state_diff.storage_diffs),
                                declared_classes: classes,
                                // TODO(dan): fix once nonces are available.
                                nonces: vec![],
                            };
                            yield Ok((current_block_number, state_diff_forward));
                            current_block_number = current_block_number.next();
                        }
                        Err(err) => {
                            debug!("Received error for state diff {}: {:?}.", current_block_number.0, err);
                            // TODO(dan): proper error handling.
                            match err{
                                _ => yield (Err(CentralError::ClientError(err))),
                            }
                        }
                    }
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
}
