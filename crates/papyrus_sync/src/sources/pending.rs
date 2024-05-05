#[cfg(test)]
#[path = "pending_test.rs"]
mod pending_test;

use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_client::reader::{
    PendingData,
    ReaderClientError,
    StarknetFeederGatewayClient,
    StarknetReader,
};
use starknet_client::ClientCreationError;

// TODO(dvir): add pending config.
use super::central::CentralSourceConfig;

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
}
#[cfg_attr(test, automock)]
#[async_trait]
pub trait PendingSourceTrait {
    async fn get_pending_data(&self) -> Result<PendingData, PendingError>;
}

#[async_trait]
impl<TStarknetClient: StarknetReader + Send + Sync + 'static> PendingSourceTrait
    for GenericPendingSource<TStarknetClient>
{
    async fn get_pending_data(&self) -> Result<PendingData, PendingError> {
        match self.starknet_client.pending_data().await {
            Ok(Some(pending_data)) => Ok(pending_data),
            Ok(None) => Err(PendingError::PendingBlockNotFound),
            Err(err) => Err(PendingError::ClientError(Arc::new(err))),
        }
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
            config.get_http_headers(),
            node_version,
            config.retry_config,
        )?;

        Ok(PendingSource { starknet_client: Arc::new(starknet_client) })
    }
}
