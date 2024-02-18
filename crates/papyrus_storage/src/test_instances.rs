use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{
    ContractAddress,
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::{
    EventIndexInTransactionOutput,
    ExecutionResources,
    Fee,
    MessageToL1,
    TransactionExecutionStatus,
    TransactionOffsetInBlock,
};
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::body::events::{
    ThinDeclareTransactionOutput,
    ThinDeployAccountTransactionOutput,
    ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput,
    ThinL1HandlerTransactionOutput,
    ThinTransactionOutput,
};
use crate::body::TransactionIndex;
use crate::compression_utils::IsCompressed;
use crate::header::{StorageBlockHeader, StorageBlockHeaderV0};
use crate::mmap_file::LocationInFile;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::version::Version;
use crate::{EventIndex, MarkerKind, OffsetKind};

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
        pub n_transactions: Option<usize>,
        pub n_events: Option<usize>,
    }
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
        State = 2,
        CompiledClass = 3,
    }
    pub enum OffsetKind {
        ThinStateDiff = 0,
        ContractClass = 1,
        Casm = 2,
        DeprecatedContractClass = 3,
    }
    pub struct ThinDeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinDeployTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinDeployAccountTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinInvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub struct ThinL1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
        pub execution_resources: ExecutionResources,
    }
    pub enum ThinTransactionOutput {
        Declare(ThinDeclareTransactionOutput) = 0,
        Deploy(ThinDeployTransactionOutput) = 1,
        DeployAccount(ThinDeployAccountTransactionOutput) = 2,
        Invoke(ThinInvokeTransactionOutput) = 3,
        L1Handler(ThinL1HandlerTransactionOutput) = 4,
    }
    struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);
    pub struct Version(pub u32);
}
