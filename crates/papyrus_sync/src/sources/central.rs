use std::collections::HashMap;
use std::sync::Arc;

use async_stream::stream;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::{future, pin_mut, TryStreamExt};
use futures_util::StreamExt;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{StateDiff, StateNumber};
use starknet_api::StarknetApiError;
use starknet_client::{
    ClientCreationError, ClientError, GenericContractClass, RetryConfig, StarknetClient,
    StarknetClientTrait, StateUpdate,
};
use tokio_stream::Stream;
use tracing::{debug, trace};

use super::stream_utils::MyStreamExt;

pub type CentralResult<T> = Result<T, CentralError>;
#[derive(Clone, Serialize, Deserialize)]
pub struct CentralSourceConfig {
    pub concurrent_requests: usize,
    pub url: String,
    pub http_headers: Option<HashMap<String, String>>,
    pub retry_config: RetryConfig,
}
pub struct GenericCentralSource<TStarknetClient: StarknetClientTrait + Send + Sync> {
    pub concurrent_requests: usize,
    pub starknet_client: Arc<TStarknetClient>,
    pub storage_reader: StorageReader,
}

#[derive(thiserror::Error, Debug)]
pub enum CentralError {
    #[error(transparent)]
    ClientCreation(#[from] ClientCreationError),
    #[error(transparent)]
    ClientError(#[from] Arc<ClientError>),
    #[error("Could not find a state update.")]
    StateUpdateNotFound,
    #[error("Could not find a class definitions.")]
    ClassNotFound,
    #[error("Could not find a block with block number {}.", block_number)]
    BlockNotFound { block_number: BlockNumber },
    #[error(transparent)]
    StarknetApiError(#[from] Arc<StarknetApiError>),
    #[error(transparent)]
    StorageError(#[from] StorageError),
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CentralSourceTrait {
    async fn get_block_marker(&self) -> Result<BlockNumber, CentralError>;
    fn stream_new_blocks(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> BlocksStream<'_>;
    fn stream_state_updates(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> StateUpdatesStream<'_>;

    async fn get_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<BlockHash>, CentralError>;
}

pub(crate) type BlocksStream<'a> = BoxStream<'a, Result<(BlockNumber, Block), CentralError>>;
type CentralStateUpdate =
    (BlockNumber, BlockHash, StateDiff, IndexMap<ClassHash, DeprecatedContractClass>);
pub(crate) type StateUpdatesStream<'a> = BoxStream<'a, CentralResult<CentralStateUpdate>>;

#[async_trait]
impl<TStarknetClient: StarknetClientTrait + Send + Sync + 'static> CentralSourceTrait
    for GenericCentralSource<TStarknetClient>
{
    async fn get_block_marker(&self) -> Result<BlockNumber, CentralError> {
        self.starknet_client
            .block_number()
            .await
            .map_err(Arc::new)?
            .map_or(Ok(BlockNumber::default()), |block_number| Ok(block_number.next()))
    }

    async fn get_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<BlockHash>, CentralError> {
        self.starknet_client
            .block(block_number)
            .await
            .map_err(Arc::new)?
            .map_or(Ok(None), |block| Ok(Some(block.block_hash)))
    }

    fn stream_state_updates(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> StateUpdatesStream<'_> {
        let mut current_block_number = initial_block_number;
        stream! {
            while current_block_number < up_to_block_number {
                let state_update_stream = self.state_update_stream(futures_util::stream::iter(
                    current_block_number.iter_up_to(up_to_block_number),
                ));
                pin_mut!(state_update_stream);
                while let Some(maybe_client_state_update) = state_update_stream.next().await {
                    let maybe_central_state_update = client_to_central_state_update(
                        current_block_number, maybe_client_state_update
                    );
                    match maybe_central_state_update {
                        Ok(central_state_update) => {
                            yield Ok(central_state_update);
                            current_block_number = current_block_number.next();
                        },
                        Err(err) => {
                            yield Err(err);
                            return;
                        }
                    }
                }
            }
        }
        .boxed()
    }

    // TODO(shahak): rename.
    fn stream_new_blocks(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> BlocksStream<'_> {
        stream! {
            // TODO(dan): add explanation.
            let mut res =
                futures_util::stream::iter(initial_block_number.iter_up_to(up_to_block_number))
                    .map(|bn| async move { (bn, self.starknet_client.block(bn).await) })
                    .buffered(self.concurrent_requests);
            while let Some((current_block_number, maybe_client_block)) = res.next().await {
                let maybe_central_block =
                    client_to_central_block(current_block_number, maybe_client_block);
                match maybe_central_block {
                    Ok(block) => {
                        yield Ok((current_block_number, block));
                    }
                    Err(err) => {
                        yield (Err(err));
                        return;
                    }
                }
            }
        }
        .boxed()
    }
}

fn client_to_central_state_update(
    current_block_number: BlockNumber,
    maybe_client_state_update: CentralResult<(
        StateUpdate,
        IndexMap<ClassHash, GenericContractClass>,
    )>,
) -> CentralResult<CentralStateUpdate> {
    match maybe_client_state_update {
        Ok((state_update, mut declared_classes)) => {
            // Destruct the state diff to avoid partial move.
            let starknet_client::StateDiff {
                storage_diffs,
                deployed_contracts,
                declared_classes: declared_class_hashes,
                old_declared_contracts: old_declared_contract_hashes,
                nonces,
                replaced_classes,
            } = state_update.state_diff;

            // Seperate the declared classes to new classes, old classes and classes of deployed
            // contracts (both new and old).
            let n_declared_classes = declared_class_hashes.len();
            let mut deprecated_classes = declared_classes.split_off(n_declared_classes);
            let n_deprecated_declared_classes = old_declared_contract_hashes.len();
            let deployed_contract_class_definitions =
                deprecated_classes.split_off(n_deprecated_declared_classes);

            let state_diff = StateDiff {
                deployed_contracts: IndexMap::from_iter(
                    deployed_contracts.iter().map(|dc| (dc.address, dc.class_hash)),
                ),
                storage_diffs: IndexMap::from_iter(storage_diffs.into_iter().map(
                    |(address, entries)| {
                        (address, entries.into_iter().map(|se| (se.key, se.value)).collect())
                    },
                )),
                declared_classes: declared_classes
                    .into_iter()
                    .map(|(class_hash, generic_class)| {
                        (class_hash, generic_class.to_cairo1().expect("Expected Cairo1 class."))
                    })
                    .zip(
                        declared_class_hashes
                            .into_iter()
                            .map(|hash_entry| hash_entry.compiled_class_hash),
                    )
                    .map(|((class_hash, class), compiled_class_hash)| {
                        (class_hash, (compiled_class_hash, class))
                    })
                    .collect(),
                deprecated_declared_classes: deprecated_classes
                    .into_iter()
                    .map(|(class_hash, generic_class)| {
                        (class_hash, generic_class.to_cairo0().expect("Expected Cairo0 class."))
                    })
                    .collect(),
                nonces,
                replaced_classes: replaced_classes
                    .into_iter()
                    .map(|replaced_class| (replaced_class.address, replaced_class.class_hash))
                    .collect(),
            };
            // Filter out deployed contracts of new classes because since 0.11 new classes can not
            // be implicitly declared by deployment.
            let deployed_contract_class_definitions = deployed_contract_class_definitions
                .into_iter()
                .filter_map(|(class_hash, contract_class)| match contract_class {
                    GenericContractClass::Cairo0ContractClass(deprecated_contract_class) => {
                        Some((class_hash, deprecated_contract_class.into()))
                    }
                    GenericContractClass::Cairo1ContractClass(_) => None,
                    GenericContractClass::APIContractClass(_) => None,
                    GenericContractClass::APIDeprecatedContractClass(
                        api_deprecated_contract_class,
                    ) => Some((class_hash, api_deprecated_contract_class)),
                })
                .collect();
            let block_hash = state_update.block_hash;
            debug!(
                "Received new state update of block {current_block_number} with hash {block_hash}."
            );
            trace!(
                "State diff: {state_diff:?}, deployed_contract_class_definitions: \
                 {deployed_contract_class_definitions:?}."
            );
            Ok((current_block_number, block_hash, state_diff, deployed_contract_class_definitions))
        }
        Err(err) => {
            debug!("Received error for state diff {}: {:?}.", current_block_number, err);
            Err(err)
        }
    }
}

fn client_to_central_block(
    current_block_number: BlockNumber,
    maybe_client_block: Result<Option<starknet_client::Block>, ClientError>,
) -> CentralResult<Block> {
    let res = match maybe_client_block {
        Ok(Some(block)) => {
            debug!("Received new block {current_block_number} with hash {}.", block.block_hash);
            trace!("Block: {block:#?}.");
            Block::try_from(block).map_err(|err| CentralError::ClientError(Arc::new(err)))
        }
        Ok(None) => Err(CentralError::BlockNotFound { block_number: current_block_number }),
        Err(err) => Err(CentralError::ClientError(Arc::new(err))),
    };
    match res {
        Ok(block) => Ok(block),
        Err(err) => {
            debug!("Received error for block {}: {:?}.", current_block_number, err);
            Err(err)
        }
    }
}

// Given a class hash, returns the corresponding class definition.
// First tries to retrieve the class from the storage.
// If not found in the storage, the class is downloaded.
async fn download_class_if_necessary<TStarknetClient: StarknetClientTrait>(
    class_hash: ClassHash,
    starknet_client: Arc<TStarknetClient>,
    storage_reader: StorageReader,
) -> CentralResult<Option<GenericContractClass>> {
    let txn = storage_reader.begin_ro_txn()?;
    let state_reader = txn.get_state_reader()?;
    let block_number = txn.get_state_marker()?;
    let state_number = StateNumber::right_after_block(block_number);

    // Check declared classes.
    if let Ok(Some(class)) = state_reader.get_class_definition_at(state_number, &class_hash) {
        trace!("Class {:?} retrieved from storage.", class_hash);
        return Ok(Some(GenericContractClass::APIContractClass(class)));
    };

    // Check deprecated classes.
    if let Ok(Some(class)) =
        state_reader.get_deprecated_class_definition_at(state_number, &class_hash)
    {
        trace!("Deprecated class {:?} retrieved from storage.", class_hash);
        return Ok(Some(GenericContractClass::APIDeprecatedContractClass(class)));
    }

    // Class not found in storage - download.
    trace!("Downloading class {:?}.", class_hash);
    Ok(starknet_client.class_by_hash(class_hash).await.map_err(Arc::new)?)
}

impl<TStarknetClient: StarknetClientTrait + Send + Sync + 'static>
    GenericCentralSource<TStarknetClient>
{
    fn state_update_stream(
        &self,
        block_number_stream: impl Stream<Item = BlockNumber> + Send + Sync + 'static,
    ) -> impl Stream<Item = CentralResult<(StateUpdate, IndexMap<ClassHash, GenericContractClass>)>>
    {
        // Stream the state updates.
        let starknet_client = self.starknet_client.clone();
        let (state_updates0, mut state_updates1) = block_number_stream
            .map(move |block_number| {
                let starknet_client = starknet_client.clone();
                async move { starknet_client.state_update(block_number).await }
            })
            .buffered(self.concurrent_requests)
            // Client error is not cloneable.
            .map_err(Arc::new)
            .fanout(self.concurrent_requests);

        // Stream the declared and deployed classes.
        let starknet_client = self.starknet_client.clone();
        let storage_reader = self.storage_reader.clone();
        let mut flat_classes = state_updates0
            // In case state_updates1 contains a ClientError, we yield it and break - without
            // evaluating flat_classes.
            .filter_map(|state_update| future::ready(state_update.ok()))
            .filter_map(future::ready)
            .map(|state_update| state_update.state_diff.class_hashes())
            .flat_map(futures::stream::iter)
            .map(move |class_hash| {
                let starknet_client = starknet_client.clone();
                let storage_reader = storage_reader.clone();
                async move { (class_hash, download_class_if_necessary(class_hash, starknet_client, storage_reader).await) }
            })
            .buffered(self.concurrent_requests);

        let res_stream = stream! {
            while let Some(maybe_state_update) = state_updates1.next().await {
                // Get the next state update.
                let state_update = match maybe_state_update {
                    Ok(Some(state_update)) => state_update,
                    Ok(None) => {
                        yield (Err(CentralError::StateUpdateNotFound));
                        break;
                    }
                    Err(err) => {
                        yield (Err(CentralError::ClientError(err)));
                        break;
                    }
                };

                // Get the next state declared and deployed classes.
                let len = state_update.state_diff.class_hashes().len();
                let classes: Option<Result<IndexMap<ClassHash, GenericContractClass>, _>> =
                    flat_classes.take_n(len).await.map(|v| {
                        v.into_iter()
                            .map(|(class_hash, class)| match class {
                                Ok(Some(class)) => Ok((class_hash, class)),
                                Ok(None) => Err(CentralError::StateUpdateNotFound),
                                Err(err) => Err(err),
                            })
                            .collect()
                    });
                match classes {
                    Some(Ok(classes)) => yield (Ok((state_update, classes))),
                    Some(Err(err)) => yield (Err(err)),
                    None => yield (Err(CentralError::ClassNotFound)),
                }
            }
        };
        res_stream
    }
}

pub type CentralSource = GenericCentralSource<StarknetClient>;

impl CentralSource {
    pub fn new(
        config: CentralSourceConfig,
        storage_reader: StorageReader,
    ) -> Result<CentralSource, ClientCreationError> {
        let starknet_client =
            StarknetClient::new(&config.url, config.http_headers, config.retry_config)?;
        Ok(CentralSource {
            concurrent_requests: config.concurrent_requests,
            starknet_client: Arc::new(starknet_client),
            storage_reader,
        })
    }
}
