//! This module contains structs for representing a broadcasted transaction.
//!
//! A broadcasted transaction is a transaction that wasn't accepted yet to Starknet.
//!
//! The broadcasted transaction follows the same structure as described in the [`Starknet specs`]
//!
//! [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json

#[cfg(test)]
#[path = "broadcasted_transaction_test.rs"]
mod broadcasted_transaction_test;

use papyrus_storage::db::serialization::StorageSerdeError;
use serde::{Deserialize, Serialize};
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::transaction::{
    AccountDeploymentData,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    TransactionVersion,
};
use starknet_client::writer::objects::transaction as client_transaction;
use starknet_client::writer::objects::transaction::DeprecatedContractClass;

use super::state::ContractClass;
use super::transaction::{DeployAccountTransaction, InvokeTransaction, ResourceBoundsMapping};
use crate::compression_utils::compress_and_encode;

/// Transactions that are ready to be broadcasted to the network and are not included in a block.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BroadcastedTransaction {
    #[serde(rename = "DECLARE")]
    Declare(BroadcastedDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransaction),
}

/// A broadcasted declare transaction.
///
/// This transaction is equivalent to the component DECLARE_TXN in the
/// [`Starknet specs`] without the V0 variant and with a contract class (DECLARE_TXN allows having
/// either a contract class or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(tag = "version")]
pub enum BroadcastedDeclareTransaction {
    #[serde(rename = "0x1")]
    V1(BroadcastedDeclareV1Transaction),
    #[serde(rename = "0x2")]
    V2(BroadcastedDeclareV2Transaction),
    #[serde(rename = "0x3")]
    V3(BroadcastedDeclareV3Transaction),
}

/// A broadcasted declare transaction of a Cairo-v0 contract.
///
/// This transaction is equivalent to the component DECLARE_TXN_V1 in the
/// [`Starknet specs`] with a contract class (DECLARE_TXN_V1 allows having either a contract class
/// or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV1Transaction {
    pub r#type: DeclareType,
    pub contract_class: DeprecatedContractClass,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
}

/// A broadcasted declare transaction of a Cairo-v1 contract.
///
/// This transaction is equivalent to the component DECLARE_TXN_V2 in the
/// [`Starknet specs`] with a contract class (DECLARE_TXN_V2 allows having either a contract class
/// or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV2Transaction {
    pub r#type: DeclareType,
    pub contract_class: ContractClass,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV3Transaction {
    pub r#type: DeclareType,
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: ContractClass,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

/// The type field of a declare transaction. This enum serializes/deserializes into a constant
/// string.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq)]
pub enum DeclareType {
    #[serde(rename = "DECLARE")]
    #[default]
    Declare,
}

impl TryFrom<BroadcastedDeclareTransaction> for client_transaction::DeclareTransaction {
    type Error = StorageSerdeError;

    fn try_from(value: BroadcastedDeclareTransaction) -> Result<Self, Self::Error> {
        match value {
            BroadcastedDeclareTransaction::V1(declare_v1) => {
                Ok(Self::DeclareV1(client_transaction::DeclareV1Transaction {
                    contract_class: declare_v1.contract_class,
                    sender_address: declare_v1.sender_address,
                    nonce: declare_v1.nonce,
                    max_fee: declare_v1.max_fee,
                    signature: declare_v1.signature,
                    version: TransactionVersion::ONE,
                    r#type: client_transaction::DeclareType::default(),
                }))
            }
            BroadcastedDeclareTransaction::V2(declare_v2) => {
                Ok(Self::DeclareV2(client_transaction::DeclareV2Transaction {
                    contract_class: client_transaction::ContractClass {
                        compressed_sierra_program: compress_and_encode(serde_json::to_value(
                            &declare_v2.contract_class.sierra_program,
                        )?)?,
                        contract_class_version: declare_v2.contract_class.contract_class_version,
                        entry_points_by_type: declare_v2
                            .contract_class
                            .entry_points_by_type
                            .to_hash_map(),
                        abi: declare_v2.contract_class.abi,
                    },
                    compiled_class_hash: declare_v2.compiled_class_hash,
                    sender_address: declare_v2.sender_address,
                    nonce: declare_v2.nonce,
                    max_fee: declare_v2.max_fee,
                    signature: declare_v2.signature,
                    version: TransactionVersion::TWO,
                    r#type: client_transaction::DeclareType::default(),
                }))
            }
            BroadcastedDeclareTransaction::V3(declare_v3) => {
                Ok(Self::DeclareV3(client_transaction::DeclareV3Transaction {
                    contract_class: client_transaction::ContractClass {
                        compressed_sierra_program: compress_and_encode(serde_json::to_value(
                            &declare_v3.contract_class.sierra_program,
                        )?)?,
                        contract_class_version: declare_v3.contract_class.contract_class_version,
                        entry_points_by_type: declare_v3
                            .contract_class
                            .entry_points_by_type
                            .to_hash_map(),
                        abi: declare_v3.contract_class.abi,
                    },
                    resource_bounds: declare_v3.resource_bounds.into(),
                    tip: declare_v3.tip,
                    signature: declare_v3.signature,
                    nonce: declare_v3.nonce,
                    compiled_class_hash: declare_v3.compiled_class_hash,
                    sender_address: declare_v3.sender_address,
                    nonce_data_availability_mode:
                        client_transaction::ReservedDataAvailabilityMode::Reserved,
                    fee_data_availability_mode:
                        client_transaction::ReservedDataAvailabilityMode::Reserved,
                    paymaster_data: declare_v3.paymaster_data,
                    account_deployment_data: declare_v3.account_deployment_data,
                    version: TransactionVersion::THREE,
                    r#type: client_transaction::DeclareType::Declare,
                }))
            }
        }
    }
}
