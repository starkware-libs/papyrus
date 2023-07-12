use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::{
    EntryPoint as DeprecatedEntryPoint, EntryPointType as DeprecatedEntryPointType, EventAbiEntry,
    FunctionAbiEntry, StructAbiEntry,
};
use starknet_api::state::{EntryPoint, EntryPointType};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, Fee, TransactionSignature, TransactionVersion,
};

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum Transaction {
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransaction),
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(InvokeTransaction),
    #[serde(rename = "DEPRECATED_DECLARE")]
    DeclareV1(DeclareV1Transaction),
    #[serde(rename = "DECLARE")]
    DeclareV2(DeclareV2Transaction),
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeployAccountTransaction {
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    #[serde(default)]
    pub version: TransactionVersion,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InvokeTransaction {
    pub calldata: Calldata,
    pub sender_address: ContractAddress,
    #[serde(default)]
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclareV1Transaction {
    pub contract_class: DeprecatedContractClass,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclareV2Transaction {
    pub contract_class: ContractClass,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
}

// The only difference between this and ContractClass in starknet_api (in the
// deprecated_contract_class module) is in the program.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeprecatedContractClass {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub abi: Option<Vec<DeprecatedContractClassAbiEntry>>,
    // The program is compressed.
    #[serde(rename = "program")]
    // TODO(shahak): Create a struct for a compressed base64 value.
    pub compressed_program: String,
    pub entry_points_by_type: HashMap<DeprecatedEntryPointType, Vec<DeprecatedEntryPoint>>,
}

// The only difference between this and ContractClass in starknet_api is in the sierra_program and
// in the version.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    // TODO(shahak): Create a struct for a compressed base64 value.
    #[serde(rename = "sierra_program")]
    pub compressed_sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub abi: String,
}

// The differences between this and ContractClassAbiEntry in starknet_api are:
// 1. This enum is tagged.
// 2. There are variants for Constructor and L1Handler.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum DeprecatedContractClassAbiEntry {
    #[serde(rename = "event")]
    Event(EventAbiEntry),
    #[serde(rename = "function")]
    Function(FunctionAbiEntry),
    #[serde(rename = "constructor")]
    Constructor(FunctionAbiEntry),
    #[serde(rename = "l1_handler")]
    L1Handler(FunctionAbiEntry),
    #[serde(rename = "struct")]
    Struct(StructAbiEntry),
}

#[cfg(any(feature = "testing", test))]
use rand::Rng;
#[cfg(any(feature = "testing", test))]
use rand_chacha::ChaCha8Rng;
#[cfg(any(feature = "testing", test))]
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};
#[cfg(any(feature = "testing", test))]
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
