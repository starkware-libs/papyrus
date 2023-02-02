use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::core::GlobalRoot;

#[cfg(test)]
#[path = "base_layer_test.rs"]
mod base_layer_test;

pub mod ethereum_base_layer_contract;

// TODO(yair): Get the block hash instead of the state root once the functionality is implemented in
// Starknet.

/// Interface for getting data from the Starknet base contract.
#[async_trait]
pub trait BaseLayerContract {
    type Error;
    /// Get the latest Starknet block that is proved on the base layer.
    async fn latest_proved_block(
        &self,
        min_confirmations: Option<u64>,
    ) -> Result<(BlockNumber, GlobalRoot), Self::Error>;
}
