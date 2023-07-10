use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use papyrus_base_layer::BaseLayerContract;
use starknet_api::block::{BlockHash, BlockNumber};

pub type BaseLayerSource = EthereumBaseLayerContract;

#[derive(thiserror::Error, Debug)]
pub enum BaseLayerError {
    #[error("Base layer error: {0}")]
    BaseLayerContractError(Box<dyn BaseLayerSourceErrorTrait>),
}

pub trait BaseLayerSourceErrorTrait: std::error::Error + Sync + Send {}

impl<Error: std::error::Error + Sync + Send> BaseLayerSourceErrorTrait for Error {}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BaseLayerSourceTrait {
    async fn latest_proved_block(&self)
    -> Result<Option<(BlockNumber, BlockHash)>, BaseLayerError>;
}

#[async_trait]
impl<
    Error: std::error::Error + 'static + Sync + Send,
    BaseLayerSource: BaseLayerContract<Error = Error> + Sync + Send,
> BaseLayerSourceTrait for BaseLayerSource
{
    async fn latest_proved_block(
        &self,
    ) -> Result<Option<(BlockNumber, BlockHash)>, BaseLayerError> {
        self.latest_proved_block(None)
            .await
            .map_err(|e| BaseLayerError::BaseLayerContractError(Box::new(e)))
    }
}
