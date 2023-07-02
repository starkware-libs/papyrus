mod state_update_stream;

use std::collections::HashMap;
use std::sync::Arc;

use async_stream::stream;
use async_trait::async_trait;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, StateDiff};
use starknet_api::StarknetApiError;
use starknet_reader_client::{
    ClientCreationError, ClientError, GenericContractClass, RetryConfig, StarknetClient,
    StarknetClientTrait,
};
use tracing::{debug, trace};

use self::state_update_stream::StateUpdateStream;

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

#[derive(Clone)]
pub enum ApiContractClass {
    DeprecatedContractClass(starknet_api::deprecated_contract_class::ContractClass),
    ContractClass(starknet_api::state::ContractClass),
}

impl From<GenericContractClass> for ApiContractClass {
    fn from(value: GenericContractClass) -> Self {
        match value {
            GenericContractClass::Cairo0ContractClass(class) => {
                Self::DeprecatedContractClass(class)
            }
            GenericContractClass::Cairo1ContractClass(class) => Self::ContractClass(class.into()),
        }
    }
}

impl ApiContractClass {
    pub fn into_cairo0(self) -> CentralResult<DeprecatedContractClass> {
        match self {
            Self::DeprecatedContractClass(class) => Ok(class),
            _ => Err(CentralError::BadContractClassType),
        }
    }

    pub fn into_cairo1(self) -> CentralResult<ContractClass> {
        match self {
            Self::ContractClass(class) => Ok(class),
            _ => Err(CentralError::BadContractClassType),
        }
    }
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
    #[error("Could not find a compiled class of {}.", class_hash)]
    CompiledClassNotFound { class_hash: ClassHash },
    #[error("Could not find a block with block number {}.", block_number)]
    BlockNotFound { block_number: BlockNumber },
    #[error(transparent)]
    StarknetApiError(#[from] Arc<StarknetApiError>),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Wrong type of contract class")]
    BadContractClassType,
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

    fn stream_compiled_classes(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> CompiledClassesStream<'_>;
}

pub(crate) type BlocksStream<'a> = BoxStream<'a, Result<(BlockNumber, Block), CentralError>>;
type CentralStateUpdate =
    (BlockNumber, BlockHash, StateDiff, IndexMap<ClassHash, DeprecatedContractClass>);
pub(crate) type StateUpdatesStream<'a> = BoxStream<'a, CentralResult<CentralStateUpdate>>;
type CentralCompiledClass = (ClassHash, CompiledClassHash, CasmContractClass);
pub(crate) type CompiledClassesStream<'a> = BoxStream<'a, CentralResult<CentralCompiledClass>>;

#[async_trait]
impl<TStarknetClient: StarknetClientTrait + Send + Sync + 'static> CentralSourceTrait
    for GenericCentralSource<TStarknetClient>
{
    // Returns the block number of the latest block from the central source.
    async fn get_block_marker(&self) -> Result<BlockNumber, CentralError> {
        self.starknet_client
            .block_number()
            .await
            .map_err(Arc::new)?
            .map_or(Ok(BlockNumber::default()), |block_number| Ok(block_number.next()))
    }

    // Returns the current block hash of the given block number from the central source.
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

    // Returns a stream of state updates downloaded from the central source.
    fn stream_state_updates(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> StateUpdatesStream<'_> {
        StateUpdateStream::new(
            initial_block_number,
            up_to_block_number,
            self.starknet_client.clone(),
            self.storage_reader.clone(),
        )
        .boxed()
    }

    // TODO(shahak): rename.
    // Returns a stream of blocks downloaded from the central source.
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

    // Returns a stream of compiled classes downloaded from the central source.
    fn stream_compiled_classes(
        &self,
        initial_block_number: BlockNumber,
        up_to_block_number: BlockNumber,
    ) -> CompiledClassesStream<'_> {
        stream! {
            let txn = self.storage_reader.begin_ro_txn().map_err(CentralError::StorageError)?;
            let class_hashes_iter = initial_block_number
                .iter_up_to(up_to_block_number)
                .map(|bn| {
                    match txn.get_state_diff(bn) {
                        Err(err) => Err(CentralError::StorageError(err)),
                        // TODO(yair): Consider expecting, since the state diffs should not contain
                        // holes and we suppose to never exceed the state marker.
                        Ok(None) => Err(CentralError::StateUpdateNotFound),
                        Ok(Some(state_diff)) => Ok(state_diff),
                    }
                })
                .flat_map(|maybe_state_diff| match maybe_state_diff {
                    Ok(state_diff) => {
                        state_diff
                            .declared_classes
                            .into_iter()
                            .map(Ok)
                            .collect()
                    }
                    Err(err) => vec![Err(err)],
                });

            let mut compiled_classes = futures_util::stream::iter(class_hashes_iter)
                .map(|maybe_class_hashes| async move {
                    match maybe_class_hashes {
                        Ok((class_hash, compiled_class_hash)) => {
                            trace!("Downloading compiled class {:?}.", class_hash);
                            match self.starknet_client.compiled_class_by_hash(class_hash).await {
                                Ok(Some(compiled_class)) => Ok((class_hash, compiled_class_hash, compiled_class)),
                                Ok(None) => Err(CentralError::CompiledClassNotFound{class_hash}),
                                Err(err) => Err(CentralError::ClientError(Arc::new(err))),
                            }
                        },
                        Err(err) => Err(err),
                    }
                })
                .buffered(self.concurrent_requests);

            while let Some(maybe_compiled_class) = compiled_classes.next().await {
                match maybe_compiled_class {
                    Ok((class_hash, compiled_class_hash, compiled_class)) => {
                        yield Ok((class_hash, compiled_class_hash, compiled_class));
                    }
                    Err(err) => {
                        yield Err(err);
                        return;
                    }
                }
            }
        }
        .boxed()
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

pub type CentralSource = GenericCentralSource<StarknetClient>;

impl CentralSource {
    pub fn new(
        config: CentralSourceConfig,
        node_version: &'static str,
        storage_reader: StorageReader,
    ) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetClient::new(
            &config.url,
            config.http_headers,
            node_version,
            config.retry_config,
        )?;

        Ok(CentralSource {
            concurrent_requests: config.concurrent_requests,
            starknet_client: Arc::new(starknet_client),
            storage_reader,
        })
    }
}
