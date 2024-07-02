use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::{
    EventIndexInTransactionOutput,
    TransactionHash,
    TransactionOffsetInBlock,
};
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::body::TransactionIndex;
use crate::compression_utils::IsCompressed;
use crate::header::StorageBlockHeader;
use crate::mmap_file::LocationInFile;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::version::Version;
use crate::{EventIndex, MarkerKind, OffsetKind, TransactionMetadata};

auto_impl_get_test_instance! {
    pub struct StorageBlockHeader {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub l1_gas_price: GasPricePerToken,
        pub l1_data_gas_price: GasPricePerToken,
        pub state_root: GlobalRoot,
        pub sequencer: SequencerContractAddress,
        pub timestamp: BlockTimestamp,
        pub l1_da_mode: L1DataAvailabilityMode,
        pub state_diff_commitment: Option<StateDiffCommitment>,
        pub transaction_commitment: Option<TransactionCommitment>,
        pub event_commitment: Option<EventCommitment>,
        pub receipt_commitment: Option<ReceiptCommitment>,
        pub state_diff_length: Option<usize>,
        pub n_transactions: usize,
        pub n_events: usize,
    }

    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
    pub struct IndexedDeprecatedContractClass {
        pub block_number: BlockNumber,
        pub location_in_file: LocationInFile,
    }
    pub enum IsCompressed {
        No = 0,
        Yes = 1,
    }
    enum MarkerKind {
        Header = 0,
        Body = 1,
        Event = 2,
        State = 3,
        Class = 4,
        CompiledClass = 5,
        BaseLayerBlock = 6,
    }
    pub enum OffsetKind {
        ThinStateDiff = 0,
        ContractClass = 1,
        Casm = 2,
        DeprecatedContractClass = 3,
    }
    pub struct TransactionMetadata{
        pub tx_hash: TransactionHash,
        pub tx_location: LocationInFile,
        pub tx_output_location: LocationInFile,
    }
    struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);
    pub struct Version{
        pub major: u32,
        pub minor: u32,
    }
}
