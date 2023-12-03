//! This module contains all the different transactions that can be added to [`Starknet`] via the
//! gateway.
//!
//! Each transaction can be serialized into a JSON object that the gateway can receive through the
//! `add_transaction` HTTP method.
//!
//! [`Starknet`]: https://starknet.io/

#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    Nonce,
};
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry as DeprecatedContractClassAbiEntry,
    EntryPoint as DeprecatedEntryPoint,
    EntryPointType as DeprecatedEntryPointType,
};
use starknet_api::state::{EntryPoint, EntryPointType};
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    ResourceBoundsMapping,
    Tip,
    TransactionSignature,
    TransactionVersion,
};

// Each transaction type has a field called `type`. This field needs to be of a type that
// serializes to/deserializes from a constant string.
//
// The reason we don't solve this by having an enum of a generic transaction and let serde generate
// the `type` field through #[serde(tag)] is because we want to serialize/deserialize from the
// structs of the specific transaction types.

/// The type field of a deploy account transaction. This enum serializes/deserializes into a
/// constant string.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq)]
pub enum DeployAccountType {
    #[serde(rename = "DEPLOY_ACCOUNT")]
    #[default]
    DeployAccount,
}

/// The type field of an invoke transaction. This enum serializes/deserializes into a constant
/// string.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq)]
pub enum InvokeType {
    #[serde(rename = "INVOKE_FUNCTION")]
    #[default]
    Invoke,
}

/// The type field of a declare transaction. This enum serializes/deserializes into a constant
/// string.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq)]
pub enum DeclareType {
    #[serde(rename = "DECLARE")]
    #[default]
    Declare,
}

// This enum is required since the GW receives this field with value 0 as a reserved value. Once the
// feature will be activated this enum should be removed from here and taken from starknet-api.
#[derive(Debug, Deserialize_repr, Serialize_repr, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum ReservedDataAvailabilityMode {
    Reserved = 0,
}

/// A deploy account transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeployAccountV1Transaction {
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
    pub r#type: DeployAccountType,
}

/// A deploy account transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
// TODO(Shahak): Add tests for deploy account v3.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeployAccountV3Transaction {
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub signature: TransactionSignature,
    pub nonce_data_availability_mode: ReservedDataAvailabilityMode,
    pub fee_data_availability_mode: ReservedDataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub version: TransactionVersion,
    pub r#type: DeployAccountType,
}

/// A deploy account transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum DeployAccountTransaction {
    DeployAccountV1(DeployAccountV1Transaction),
    DeployAccountV3(DeployAccountV3Transaction),
}

/// An invoke account transaction that can be added to Starknet through the Starknet gateway.
/// The invoke is a V0 transaction.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
// TODO(Shahak): Add tests for invoke v0.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InvokeV0Transaction {
    pub calldata: Calldata,
    pub contract_address: ContractAddress,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
    pub r#type: InvokeType,
    pub entry_point_selector: EntryPointSelector,
}

/// An invoke account transaction that can be added to Starknet through the Starknet gateway.
/// The invoke is a V1 transaction.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InvokeV1Transaction {
    pub calldata: Calldata,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
    pub r#type: InvokeType,
}

/// An invoke account transaction that can be added to Starknet through the Starknet gateway.
/// The invoke is a V3 transaction.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
// TODO(Shahak): Add tests for invoke v3.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InvokeV3Transaction {
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub calldata: Calldata,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub signature: TransactionSignature,
    pub nonce_data_availability_mode: ReservedDataAvailabilityMode,
    pub fee_data_availability_mode: ReservedDataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub version: TransactionVersion,
    pub r#type: InvokeType,
}

/// An invoke transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum InvokeTransaction {
    InvokeV0(InvokeV0Transaction),
    InvokeV1(InvokeV1Transaction),
    InvokeV3(InvokeV3Transaction),
}
/// A declare transaction of a Cairo-v0 (deprecated) contract class that can be added to Starknet
/// through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclareV1Transaction {
    pub contract_class: DeprecatedContractClass,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub r#type: DeclareType,
}

/// A declare transaction of a Cairo-v1 contract class that can be added to Starknet through the
/// Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
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
    pub r#type: DeclareType,
}

/// A declare transaction of a Cairo-v1 contract class that can be added to Starknet through the
/// Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
// TODO(shahak): Add tests for declare v3.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclareV3Transaction {
    pub contract_class: ContractClass,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce_data_availability_mode: ReservedDataAvailabilityMode,
    pub fee_data_availability_mode: ReservedDataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub version: TransactionVersion,
    pub r#type: DeclareType,
}

/// A declare transaction that can be added to Starknet through the Starknet gateway.
/// It has a serialization format that the Starknet gateway accepts in the `add_transaction`
/// HTTP method.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum DeclareTransaction {
    DeclareV1(DeclareV1Transaction),
    DeclareV2(DeclareV2Transaction),
    DeclareV3(DeclareV3Transaction),
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
