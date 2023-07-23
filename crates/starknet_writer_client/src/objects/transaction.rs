//! This module contains all the different transactions that can be added to [`Starknet`] via the
//! gateway.
//!
//! Each transaction can be serialized into a JSON object that the gateway can receive through the
//! `add_transaction` HTTP method.
//!
//! [`Starknet`]: https://starknet.io/

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

/// A generic transaction that can be added to Starknet. When the transaction is serialized into a
/// JSON object, it must be in the format that the Starknet gateway expects in the
/// `add_transaction` HTTP method.
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

/// A deploy account transaction that can be added to Starknet through the Starknet gateway.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeployAccountTransaction {
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
}

/// An invoke account transaction that can be added to Starknet through the Starknet gateway.
/// The invoke is a V1 transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InvokeTransaction {
    pub calldata: Calldata,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
}

/// A declare transaction of a Cairo-v0 (deprecated) contract class that can be added to Starknet
/// through the Starknet gateway.
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

/// A declare transaction of a Cairo-v1 contract class that can be added to Starknet through the
/// Starknet gateway.
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

// The structs that are implemented here are the structs that have deviations from starknet_api.

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeprecatedContractClass {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub abi: Option<Vec<DeprecatedContractClassAbiEntry>>,
    #[serde(rename = "program")]
    // TODO(shahak): Create a struct for a compressed base64 value.
    pub compressed_program: String,
    pub entry_points_by_type: HashMap<DeprecatedEntryPointType, Vec<DeprecatedEntryPoint>>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    // TODO(shahak): Create a struct for a compressed base64 value.
    #[serde(rename = "sierra_program")]
    pub compressed_sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    pub abi: String,
}

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
