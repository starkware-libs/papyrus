#[cfg(test)]
#[path = "central_test.rs"]
mod central_test;
mod state_update_stream;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_stream::stream;
use async_trait::async_trait;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use indexmap::IndexMap;
use itertools::chain;
#[cfg(test)]
use mockall::automock;
use papyrus_common::BlockHashAndNumber;
use papyrus_config::converters::{deserialize_optional_map, serialize_optional_map};
use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::header::StarknetVersion;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use serde::{Deserialize, Serialize};
use starknet_api::block::{Block, BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkFelt;
use starknet_api::state::{ContractClass, StateDiff};
use starknet_api::StarknetApiError;
use starknet_client::reader::{
    GenericContractClass,
    ReaderClientError,
    StarknetFeederGatewayClient,
    StarknetReader,
};
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
}

#[derive(Clone)]
enum ApiContractClass {
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
    fn into_cairo0(self) -> CentralResult<DeprecatedContractClass> {
        match self {
            Self::DeprecatedContractClass(class) => Ok(class),
            _ => Err(CentralError::BadContractClassType),
        }
    }

    fn into_cairo1(self) -> CentralResult<ContractClass> {
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
}

pub(crate) type BlocksStream<'a> = BoxStream<
    'a,
    Result<(BlockNumber, Block, CentralBlockSignatureData, StarknetVersion), CentralError>,
>;
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
                block_hash: block.block_hash,
                block_number: block.block_number,
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
            self.state_update_stream_config.clone(),
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
            while let Some((current_block_number, maybe_client_block_and_signature)) =
                res.next().await
            {
                match client_to_central_block(
                    current_block_number,
                    maybe_client_block_and_signature,
                ) {
                    Ok((block, signature, version)) => {
                        yield Ok((current_block_number, block, signature, version));
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
    maybe_client_block_and_signature: Result<
        (
            Option<starknet_client::reader::Block>,
            Option<starknet_client::reader::BlockSignatureData>,
        ),
        ReaderClientError,
    >,
) -> CentralResult<(Block, CentralBlockSignatureData, StarknetVersion)> {
    let client_block_and_signature =
        maybe_client_block_and_signature.map_err(|err| CentralError::ClientError(Arc::new(err)))?;

    match client_block_and_signature {
        (Some(block), Some(signature_data)) => {
            debug!("Received new block {current_block_number} with hash {}.", block.block_hash);
            trace!("Block: {block:#?}, signature data: {signature_data:#?}.");
            let (block, version) = block
                .to_starknet_api_block_and_version()
                .map_err(|err| CentralError::ClientError(Arc::new(err)))?;
            Ok((
                block,
                CentralBlockSignatureData {
                    signature: signature_data.signature,
                    state_diff_commitment: signature_data.signature_input.state_diff_commitment,
                },
                StarknetVersion(version),
            ))
        }
        (None, Some(_)) => {
            debug!("Block {current_block_number} not found, but signature was found.");
            Err(CentralError::BlockNotFound { block_number: current_block_number })
        }
        (Some(_), None) => {
            debug!("Block {current_block_number} found, but signature was not found.");
            Err(CentralError::BlockNotFound { block_number: current_block_number })
        }
        (None, None) => {
            debug!("Block {current_block_number} not found.");
            Err(CentralError::BlockNotFound { block_number: current_block_number })
        }
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
        })
    }
}

/// A struct that holds the signature data of a block.
// TODO(yair): use SN_API type once https://github.com/starkware-libs/starknet-api/pull/134 is merged.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CentralBlockSignatureData {
    pub signature: [StarkFelt; 2],
    pub state_diff_commitment: StarkFelt,
}
