use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use test_utils::{auto_impl_get_test_instance, GetTestInstance};

use crate::deprecated::migrations::StorageBlockHeaderV0;

auto_impl_get_test_instance! {
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
