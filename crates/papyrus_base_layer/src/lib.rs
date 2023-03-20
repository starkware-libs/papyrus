use async_trait::async_trait;
use starknet_api::block::{BlockHash, BlockNumber};

#[cfg(test)]
#[path = "base_layer_test.rs"]
mod base_layer_test;

pub mod ethereum_base_layer_contract;

/// Interface for getting data from the Starknet base contract.
#[async_trait]
pub trait BaseLayerContract {
    type Error;

    /// Get the latest Starknet block that is proved on the base layer.
    /// Optionally, require minimum confirmations.
    async fn latest_proved_block(
        &self,
        min_confirmations: Option<u64>,
    ) -> Result<Option<(BlockNumber, BlockHash)>, Self::Error>;
}
