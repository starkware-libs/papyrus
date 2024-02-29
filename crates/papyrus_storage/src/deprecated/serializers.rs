use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;

use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::deprecated::migrations::StorageBlockHeaderV0;
use crate::serialization::serializers::auto_storage_serde;
#[cfg(test)]
use crate::serialization::serializers_test::{create_storage_serde_test, StorageSerdeTest};

auto_storage_serde! {
        pub struct StorageBlockHeaderV0 {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub l1_gas_price: GasPricePerToken,
        pub l1_data_gas_price: GasPricePerToken,
        pub state_root: GlobalRoot,
        pub sequencer: SequencerContractAddress,
        pub timestamp: BlockTimestamp,
        pub l1_da_mode: L1DataAvailabilityMode,
        pub transaction_commitment: TransactionCommitment,
        pub event_commitment: EventCommitment,
        pub n_transactions: usize,
        pub n_events: usize,
    }
}
