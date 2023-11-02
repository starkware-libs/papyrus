use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    EventIndexInTransactionOutput,
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
use crate::header::StarknetVersion;
use crate::mmap_file::LocationInFile;
use crate::state::data::IndexedDeprecatedContractClass;
use crate::version::Version;
use crate::{EventIndex, MarkerKind, OffsetKind};

auto_impl_get_test_instance! {
    struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);
    pub struct IndexedDeprecatedContractClass {
        pub block_number: BlockNumber,
        pub location_in_file: LocationInFile,
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
    pub struct StarknetVersion(pub String);
    pub struct ThinDeclareTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
    }
    pub struct ThinDeployTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
    }
    pub struct ThinDeployAccountTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub contract_address: ContractAddress,
        pub execution_status: TransactionExecutionStatus,
    }
    pub struct ThinInvokeTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
    }
    pub struct ThinL1HandlerTransactionOutput {
        pub actual_fee: Fee,
        pub messages_sent: Vec<MessageToL1>,
        pub events_contract_addresses: Vec<ContractAddress>,
        pub execution_status: TransactionExecutionStatus,
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
