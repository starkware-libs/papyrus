#[cfg(test)]
#[path = "central_test.rs"]
mod central_test;
mod state_update_stream;

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use async_stream::stream;
use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use indexmap::IndexMap;
use itertools::chain;
use lru::LruCache;
#[cfg(test)]
use mockall::automock;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_common::BlockHashAndNumber;
use papyrus_config::converters::{deserialize_optional_map, serialize_optional_map};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, SequencerPublicKey};
use starknet_api::crypto::Signature;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::StateDiff;
use starknet_api::StarknetApiError;
use starknet_client::reader::{ReaderClientError, StarknetFeederGatewayClient, StarknetReader};
use starknet_client::{ClientCreationError, RetryConfig};
use tracing::{debug, trace};

use self::state_update_stream::{StateUpdateStream, StateUpdateStreamConfig};

type CentralResult<T> = Result<T, CentralError>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CentralSourceConfig {
    pub concurrent_requests: usize,
    pub url: String,
    #[serde(deserialize_with = "deserialize_optional_map")]
    pub http_headers: Option<HashMap<String, String>>,
    pub max_state_updates_to_download: usize,
    pub max_state_updates_to_store_in_memory: usize,
    pub max_classes_to_download: usize,
    // TODO(dan): validate that class_cache_size is a positive integer.
    pub class_cache_size: usize,
    pub retry_config: RetryConfig,
}

impl Default for CentralSourceConfig {
    fn default() -> Self {
        CentralSourceConfig {
            concurrent_requests: 10,
            url: String::from("https://alpha-mainnet.starknet.io/"),
            http_headers: None,
            max_state_updates_to_download: 20,
            max_state_updates_to_store_in_memory: 20,
            max_classes_to_download: 20,
            class_cache_size: 100,
            retry_config: RetryConfig {
                retry_base_millis: 30,
                retry_max_delay_millis: 30000,
                max_retries: 10,
            },
        }
    }
}

impl SerializeConfig for CentralSourceConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let self_params_dump = BTreeMap::from_iter([
            ser_param(
                "concurrent_requests",
                &self.concurrent_requests,
                "Maximum number of concurrent requests to Starknet feeder-gateway for getting a \
                 type of data (for example, blocks).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "url",
                &self.url,
                "Starknet feeder-gateway URL. It should match chain_id.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "http_headers",
                &serialize_optional_map(&self.http_headers),
                "'k1:v1 k2:v2 ...' headers for SN-client.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "max_state_updates_to_download",
                &self.max_state_updates_to_download,
                "Maximum number of state updates to download at a given time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_state_updates_to_store_in_memory",
                &self.max_state_updates_to_store_in_memory,
                "Maximum number of state updates to store in memory at a given time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_classes_to_download",
                &self.max_classes_to_download,
                "Maximum number of classes to download at a given time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "class_cache_size",
                &self.class_cache_size,
                "Size of class cache, must be a positive integer.",
                ParamPrivacyInput::Public,
            ),
        ]);
        chain!(self_params_dump, append_sub_config_name(self.retry_config.dump(), "retry_config"))
            .collect()
    }
}

pub struct GenericCentralSource<TStarknetClient: StarknetReader + Send + Sync> {
    pub concurrent_requests: usize,
    pub starknet_client: Arc<TStarknetClient>,
    pub storage_reader: StorageReader,
    pub state_update_stream_config: StateUpdateStreamConfig,
    pub(crate) class_cache: Arc<Mutex<LruCache<ClassHash, ApiContractClass>>>,
    compiled_class_cache: Arc<Mutex<LruCache<ClassHash, CasmContractClass>>>,
}

#[derive(thiserror::Error, Debug)]
pub enum CentralError {
    #[error(transparent)]
    ClientCreation(#[from] ClientCreationError),
    #[error(transparent)]
    ClientError(#[from] Arc<ReaderClientError>),
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
    async fn get_latest_block(&self) -> Result<Option<BlockHashAndNumber>, CentralError>;
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

    // TODO(shahak): Remove once pending block is removed.
    async fn get_class(&self, class_hash: ClassHash) -> Result<ApiContractClass, CentralError>;

    // TODO(shahak): Remove once pending block is removed.
    async fn get_compiled_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<CasmContractClass, CentralError>;

    async fn get_sequencer_pub_key(&self) -> Result<SequencerPublicKey, CentralError>;
}

pub(crate) type BlocksStream<'a> =
    BoxStream<'a, Result<(BlockNumber, Block, BlockSignature), CentralError>>;
type CentralStateUpdate =
    (BlockNumber, BlockHash, StateDiff, IndexMap<ClassHash, DeprecatedContractClass>);
pub(crate) type StateUpdatesStream<'a> = BoxStream<'a, CentralResult<CentralStateUpdate>>;
type CentralCompiledClass = (ClassHash, CompiledClassHash, CasmContractClass);
pub(crate) type CompiledClassesStream<'a> = BoxStream<'a, CentralResult<CentralCompiledClass>>;

#[async_trait]
impl<TStarknetClient: StarknetReader + Send + Sync + 'static> CentralSourceTrait
    for GenericCentralSource<TStarknetClient>
{
    // Returns the block hash and the block number of the latest block from the central source.
    async fn get_latest_block(&self) -> Result<Option<BlockHashAndNumber>, CentralError> {
        self.starknet_client.latest_block().await.map_err(Arc::new)?.map_or(Ok(None), |block| {
            Ok(Some(BlockHashAndNumber {
                block_hash: block.block_hash(),
                block_number: block.block_number(),
            }))
        })
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
            .map_or(Ok(None), |block| Ok(Some(block.block_hash())))
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
            self.state_update_stream_config.clone(),
            self.class_cache.clone(),
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
                    .map(|bn| async move {
                        let block_and_signature = futures_util::try_join!(
                            self.starknet_client.block(bn),
                            self.starknet_client.block_signature(bn)
                        );
                        (bn, block_and_signature)
                    })
                    .buffered(self.concurrent_requests);
            while let Some((current_block_number, maybe_client_block)) = res.next().await
            {
                let maybe_central_block =
                    client_to_central_block(current_block_number, maybe_client_block);
                match maybe_central_block {
                    Ok((block, signature)) => {
                        yield Ok((current_block_number, block, signature));
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
                            let compiled_class = self.get_compiled_class(class_hash).await?;
                            Ok((class_hash, compiled_class_hash, compiled_class))
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

    async fn get_class(&self, class_hash: ClassHash) -> Result<ApiContractClass, CentralError> {
        // TODO(shahak): Fix code duplication with StateUpdatesStream.
        {
            let mut class_cache = self.class_cache.lock().expect("Failed to lock class cache.");
            if let Some(class) = class_cache.get(&class_hash) {
                return Ok(class.clone());
            }
        }
        let client_class =
            self.starknet_client.class_by_hash(class_hash).await.map_err(Arc::new)?;
        match client_class {
            None => Err(CentralError::ClassNotFound),
            Some(class) => {
                {
                    let mut class_cache =
                        self.class_cache.lock().expect("Failed to lock class cache.");
                    class_cache.put(class_hash, class.clone().into());
                }
                Ok(class.into())
            }
        }
    }

    async fn get_compiled_class(
        &self,
        class_hash: ClassHash,
    ) -> Result<CasmContractClass, CentralError> {
        {
            let mut compiled_class_cache =
                self.compiled_class_cache.lock().expect("Failed to lock class cache.");
            if let Some(class) = compiled_class_cache.get(&class_hash) {
                return Ok(class.clone());
            }
        }
        match self.starknet_client.compiled_class_by_hash(class_hash).await {
            Ok(Some(compiled_class)) => {
                let mut compiled_class_cache =
                    self.compiled_class_cache.lock().expect("Failed to lock class cache.");
                compiled_class_cache.put(class_hash, compiled_class.clone());
                Ok(compiled_class)
            }
            Ok(None) => Err(CentralError::CompiledClassNotFound { class_hash }),
            Err(err) => Err(CentralError::ClientError(Arc::new(err))),
        }
    }

    async fn get_sequencer_pub_key(&self) -> Result<SequencerPublicKey, CentralError> {
        Ok(self.starknet_client.sequencer_pub_key().await.map_err(Arc::new)?)
    }
}

fn client_to_central_block(
    current_block_number: BlockNumber,
    maybe_client_block: Result<
        (
            Option<starknet_client::reader::BlockOrDeprecated>,
            Option<starknet_client::reader::BlockSignatureData>,
        ),
        ReaderClientError,
    >,
) -> CentralResult<(Block, BlockSignature)> {
    match maybe_client_block {
        Ok((Some(block), Some(signature_data))) => {
            debug!("Received new block {current_block_number} with hash {}.", block.block_hash());
            trace!("Block: {block:#?}, signature data: {signature_data:#?}.");
            let block = block
                .to_starknet_api_block_and_version(
                    signature_data.signature_input.state_diff_commitment,
                )
                .map_err(|err| CentralError::ClientError(Arc::new(err)))?;
            Ok((
                block,
                BlockSignature(Signature {
                    r: signature_data.signature[0],
                    s: signature_data.signature[1],
                }),
            ))
        }
        Ok((None, Some(_))) => {
            debug!("Block {current_block_number} not found, but signature was found.");
            Err(CentralError::BlockNotFound { block_number: current_block_number })
        }
        Ok((Some(_), None)) => {
            debug!("Block {current_block_number} found, but signature was not found.");
            Err(CentralError::BlockNotFound { block_number: current_block_number })
        }
        Ok((None, None)) => {
            debug!("Block {current_block_number} not found.");
            Err(CentralError::BlockNotFound { block_number: current_block_number })
        }
        Err(err) => Err(CentralError::ClientError(Arc::new(err))),
    }
}

pub type CentralSource = GenericCentralSource<StarknetFeederGatewayClient>;

impl CentralSource {
    pub fn new(
        config: CentralSourceConfig,
        node_version: &'static str,
        storage_reader: StorageReader,
    ) -> Result<CentralSource, ClientCreationError> {
        let starknet_client = StarknetFeederGatewayClient::new(
            &config.url,
            config.http_headers,
            node_version,
            config.retry_config,
        )?;

        Ok(CentralSource {
            concurrent_requests: config.concurrent_requests,
            starknet_client: Arc::new(starknet_client),
            storage_reader,
            state_update_stream_config: StateUpdateStreamConfig {
                max_state_updates_to_download: config.max_state_updates_to_download,
                max_state_updates_to_store_in_memory: config.max_state_updates_to_store_in_memory,
                max_classes_to_download: config.max_classes_to_download,
            },
            class_cache: Arc::from(Mutex::new(LruCache::new(
                NonZeroUsize::new(config.class_cache_size)
                    .expect("class_cache_size should be a positive integer."),
            ))),
            compiled_class_cache: Arc::from(Mutex::new(LruCache::new(
                NonZeroUsize::new(config.class_cache_size)
                    .expect("class_cache_size should be a positive integer."),
            ))),
        })
    }
}
