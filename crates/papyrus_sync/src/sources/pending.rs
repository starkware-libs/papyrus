#[cfg(test)]
#[path = "pending_test.rs"]
mod pending_test;

use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_common::pending_classes::{PendingClass, PendingClasses, PendingClassesTrait};
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass as Sierra;
use starknet_client::reader::{
    DeclaredClassHashEntry,
    GenericContractClass,
    PendingData,
    ReaderClientError,
    StarknetFeederGatewayClient,
    StarknetReader,
};
use starknet_client::ClientCreationError;
use tokio::sync::RwLock;
use tokio::try_join;
use tracing::{debug, trace};

// TODO(dvir): add pending config.
use super::central::CentralSourceConfig;

type PendingResult<T> = Result<T, PendingError>;

pub struct GenericPendingSource<TStarknetClient: StarknetReader + Send + Sync> {
    pub starknet_client: Arc<TStarknetClient>,
}

#[derive(thiserror::Error, Debug)]
pub enum PendingError {
    #[error(transparent)]
    ClientCreation(#[from] ClientCreationError),
    #[error(transparent)]
    ClientError(#[from] Arc<ReaderClientError>),
    #[error("Pending block not found")]
    PendingBlockNotFound,
    #[error("Could not find a class definitions of {class_hash}.")]
    ClassNotFound { class_hash: ClassHash },
    #[error("Could not find a compiled class of {class_hash}.")]
    CompiledClassNotFound { class_hash: ClassHash },
    #[error("Got unexpted class type.")]
    BadContractClassType,
}
#[cfg_attr(test, automock)]
#[async_trait]
pub trait PendingSourceTrait {
    async fn get_pending_data(&self) -> PendingResult<PendingData>;

    async fn add_pending_deprecated_class(
        &self,
        class_hash: ClassHash,
        pending_classes: Arc<RwLock<PendingClasses>>,
    ) -> PendingResult<()>;

    async fn add_pending_class(
        &self,
        class_hashes: DeclaredClassHashEntry,
        pending_classes: Arc<RwLock<PendingClasses>>,
    ) -> PendingResult<()>;
}

#[async_trait]
impl<TStarknetClient: StarknetReader + Send + Sync + 'static> PendingSourceTrait
    for GenericPendingSource<TStarknetClient>
{
    async fn get_pending_data(&self) -> PendingResult<PendingData> {
        match self.starknet_client.pending_data().await {
            Ok(Some(pending_data)) => {
                debug!("Received new pending data.");
                trace!("Pending data: {pending_data:#?}.");
                Ok(pending_data)
            }
            Ok(None) => Err(PendingError::PendingBlockNotFound),
            Err(err) => Err(PendingError::ClientError(Arc::new(err))),
        }
    }

    async fn add_pending_deprecated_class(
        &self,
        class_hash: ClassHash,
        pending_classes: Arc<RwLock<PendingClasses>>,
    ) -> PendingResult<()> {
        let deprecated_class = match self.starknet_client.class_by_hash(class_hash).await {
            Ok(Some(pending_deprecated_class)) => {
                let pending_deprecated_class = into_cairo0(pending_deprecated_class)?;
                debug!("Received new pending deprecated class.");
                trace!("Pending deprecated class: {pending_deprecated_class:#?}.");
                pending_deprecated_class
            }
            Ok(None) => return Err(PendingError::CompiledClassNotFound { class_hash }),
            Err(err) => return Err(PendingError::ClientError(Arc::new(err))),
        };
        debug!("Adding pending deprecated class with hash {class_hash} to pending classes.");
        pending_classes.write().await.add_class(class_hash, PendingClass::Cairo0(deprecated_class));
        Ok(())
    }

    async fn add_pending_class(
        &self,
        class_hashes: DeclaredClassHashEntry,
        pending_classes: Arc<RwLock<PendingClasses>>,
    ) -> PendingResult<()> {
        let DeclaredClassHashEntry { class_hash, compiled_class_hash } = class_hashes;
        let sierra_fut = self.starknet_client.class_by_hash(class_hash);
        let casm_fut =
            self.starknet_client.compiled_class_by_hash(ClassHash(compiled_class_hash.0));

        let (sierra, casm) = try_join!(sierra_fut, casm_fut).map_err(Arc::new)?;

        match sierra {
            Some(sierra) => {
                let sierra = into_cairo1(sierra)?;
                debug!("Received new pending sierra.");
                trace!("Pending sierra: {sierra:#?}.");
                debug!("Adding pending sierra with hash {class_hash} to pending classes.");
                pending_classes.write().await.add_class(class_hash, PendingClass::Cairo1(sierra));
            }
            None => return Err(PendingError::CompiledClassNotFound { class_hash }),
        };

        match casm {
            Some(casm) => {
                debug!("Received new pending casm.");
                trace!("Pending casm: {casm:#?}.");
                debug!("Adding pending casm with hash {class_hash} to pending classes.");
                pending_classes.write().await.add_casm(class_hash, casm);
            }
            None => return Err(PendingError::ClassNotFound { class_hash }),
        };
        Ok(())
    }
}

pub type PendingSource = GenericPendingSource<StarknetFeederGatewayClient>;

impl PendingSource {
    pub fn new(
        config: CentralSourceConfig,
        node_version: &'static str,
    ) -> Result<PendingSource, ClientCreationError> {
        let starknet_client = StarknetFeederGatewayClient::new(
            &config.url,
            config.http_headers,
            node_version,
            config.retry_config,
        )?;

        Ok(PendingSource { starknet_client: Arc::new(starknet_client) })
    }
}

// Copied from the cental source to prevent dependency.
// TODO(dvir): consider move this to the client or higher (also move from the central source).
fn into_cairo0(contract_class: GenericContractClass) -> PendingResult<DeprecatedContractClass> {
    match contract_class {
        GenericContractClass::Cairo0ContractClass(class) => Ok(class),
        _ => Err(PendingError::BadContractClassType),
    }
}

fn into_cairo1(contract_class: GenericContractClass) -> PendingResult<Sierra> {
    match contract_class {
        GenericContractClass::Cairo1ContractClass(class) => Ok(class.into()),
        _ => Err(PendingError::BadContractClassType),
    }
}
