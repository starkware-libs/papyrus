use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use papyrus_base_layer::BaseLayerContract;
use starknet_api::block::{BlockHash, BlockNumber};

pub type EthereumBaseLayerSource = EthereumBaseLayerContract;

#[derive(thiserror::Error, Debug)]
pub enum BaseLayerSourceError {
    #[error("Base layer error: {0}")]
    BaseLayerContractError(Box<dyn BaseLayerSourceErrorTrait>),
    #[error("Base layer source creation error: {0}.")]
    BaseLayerSourceCreationError(String),
}

pub trait BaseLayerSourceErrorTrait: std::error::Error + Sync + Send {}

impl<Error: std::error::Error + Sync + Send> BaseLayerSourceErrorTrait for Error {}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BaseLayerSourceTrait {
    async fn latest_proved_block(
        &self,
    ) -> Result<Option<(BlockNumber, BlockHash)>, BaseLayerSourceError>;
}

#[async_trait]
impl<
    Error: std::error::Error + 'static + Sync + Send,
    BaseLayerSource: BaseLayerContract<Error = Error> + Sync + Send,
> BaseLayerSourceTrait for BaseLayerSource
{
    async fn latest_proved_block(
        &self,
    ) -> Result<Option<(BlockNumber, BlockHash)>, BaseLayerSourceError> {
        self.latest_proved_block(None)
            .await
            .map_err(|e| BaseLayerSourceError::BaseLayerContractError(Box::new(e)))
    }
}
