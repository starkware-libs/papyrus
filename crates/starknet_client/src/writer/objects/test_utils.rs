use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::deprecated_contract_class::{
    EntryPoint as DeprecatedEntryPoint,
    EntryPointType as DeprecatedEntryPointType,
    EventAbiEntry,
    FunctionAbiEntry,
    StructAbiEntry,
};
use starknet_api::transaction::TransactionHash;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::writer::objects::response::{
    DeployAccountResponse, InvokeResponse, SuccessfulStarknetErrorCode,
};
use crate::writer::objects::transaction::{
    DeprecatedContractClass,
    DeprecatedContractClassAbiEntry,
};

auto_impl_get_test_instance! {
    pub struct DeprecatedContractClass {
        pub abi: Option<Vec<DeprecatedContractClassAbiEntry>>,
        pub compressed_program: String,
        pub entry_points_by_type: HashMap<DeprecatedEntryPointType, Vec<DeprecatedEntryPoint>>,
    }
    pub enum DeprecatedContractClassAbiEntry {
        Event(EventAbiEntry) = 0,
        Function(FunctionAbiEntry) = 1,
        Constructor(FunctionAbiEntry) = 2,
        L1Handler(FunctionAbiEntry) = 3,
        Struct(StructAbiEntry) = 4,
    }
    pub struct InvokeResponse {
        pub code: SuccessfulStarknetErrorCode,
        pub transaction_hash: TransactionHash,
    }
    pub struct DeployAccountResponse {
        pub code: SuccessfulStarknetErrorCode,
        pub transaction_hash: TransactionHash,
        pub address: ContractAddress,
    }
    pub enum SuccessfulStarknetErrorCode {
        TransactionReceived = 0,
    }
}
