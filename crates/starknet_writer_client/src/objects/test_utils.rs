use std::collections::HashMap;

use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::{
    EntryPoint as DeprecatedEntryPoint, EntryPointType as DeprecatedEntryPointType, EventAbiEntry,
    FunctionAbiEntry, StructAbiEntry,
};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, Fee, TransactionSignature, TransactionVersion,
};
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

use crate::objects::transaction::{
    DeclareV1Transaction, DeployAccountTransaction, DeprecatedContractClass,
    DeprecatedContractClassAbiEntry, InvokeTransaction,
};

auto_impl_get_test_instance! {
    pub struct DeployAccountTransaction {
        pub contract_address_salt: ContractAddressSalt,
        pub class_hash: ClassHash,
        pub constructor_calldata: Calldata,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub version: TransactionVersion,
    }
    pub struct InvokeTransaction {
        pub calldata: Calldata,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub version: TransactionVersion,
    }
    pub struct DeclareV1Transaction {
        pub contract_class: DeprecatedContractClass,
        pub sender_address: ContractAddress,
        pub nonce: Nonce,
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
    }
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
}
