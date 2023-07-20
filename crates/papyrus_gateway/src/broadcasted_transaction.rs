//! This module contains structs for representing a broadcasted transaction.
//!
//! A broadcasted transaction is a transaction that wasn't accepted yet to Starknet.
//!
//! The broadcasted transaction follows the same structure as described in the [`Starknet specs`]
//!
//! [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json

use papyrus_storage::compression_utils::serialize_and_compress;
use papyrus_storage::db::serialization::{StorageSerde, StorageSerdeError};
use serde::{Deserialize, Serialize};
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::transaction::{Fee, TransactionSignature, TransactionVersion};
use starknet_writer_client::objects::transaction::{
    ContractClass as ClientContractClass, DeclareV1Transaction,
    DeclareV2Transaction as ClientDeclareV2Transaction, DeployAccountTransaction,
    InvokeTransaction, Transaction as ClientTransaction,
};

use crate::v0_3_0::state::ContractClass;

/// A generic broadcasted transaction.
///
/// This transaction is equivalent to the component BROADCASTED_TXN in the [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum BroadcastedTransaction {
    #[serde(rename = "DECLARE")]
    Declare(BroadcastedDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(BroadcastedDeployAccountTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(BroadcastedInvokeTransaction),
}

/// A broadcasted declare transaction.
///
/// This transaction is equivalent to the component BROADCASTED_DECLARE_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum BroadcastedDeclareTransaction {
    DeclareV1(BroadcastedDeclareV1Transaction),
    DeclareV2(BroadcastedDeclareV2Transaction),
}

/// A broadcasted deploy account transaction.
///
/// This transaction is equivalent to the component BROADCASTED_DEPLOY_ACCOUNT_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
pub type BroadcastedDeployAccountTransaction = DeployAccountTransaction;

/// A broadcasted invoke transaction.
///
/// This transaction is equivalent to the component BROADCASTED_INVOKE_TXN in the
/// [`Starknet specs`], except that invoke v0 is not supported and the invoke is assumed to be of
/// type BROADCASTED_INVOKE_TXN_V1.
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
pub type BroadcastedInvokeTransaction = InvokeTransaction;

// BroadcastedDeclareV2Transaction is not from starknet_writer_client because the broadcasted
// declare v2 has slight alterations from the client declare v2. We define our own
// BroadcastedDeclareV2Transaction further below.
/// A broadcasted declare transaction of a Cairo-v0 contract.
///
/// This transaction is equivalent to the component BROADCASTED_DECLARE_TXN_V1 in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
pub type BroadcastedDeclareV1Transaction = DeclareV1Transaction;

// The only difference between this and DeclareV2Transaction in starknet_writer_client is the
// type of contract_class.
/// A broadcasted declare transaction of a Cairo-v1 contract.
///
/// This transaction is equivalent to the component BROADCASTED_DECLARE_TXN_V2 in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct BroadcastedDeclareV2Transaction {
    pub contract_class: ContractClass,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
}

impl TryFrom<BroadcastedTransaction> for ClientTransaction {
    type Error = ConversionError;

    fn try_from(value: BroadcastedTransaction) -> Result<Self, Self::Error> {
        match value {
            BroadcastedTransaction::DeployAccount(deploy_account) => {
                Ok(Self::DeployAccount(deploy_account))
            }
            BroadcastedTransaction::Invoke(invoke) => Ok(Self::Invoke(invoke)),
            BroadcastedTransaction::Declare(declare) => match declare {
                BroadcastedDeclareTransaction::DeclareV1(declare_v1) => {
                    Ok(Self::DeclareV1(declare_v1))
                }
                BroadcastedDeclareTransaction::DeclareV2(declare_v2) => {
                    Ok(Self::DeclareV2(ClientDeclareV2Transaction {
                        contract_class: ClientContractClass {
                            compressed_sierra_program: compress(
                                &declare_v2.contract_class.sierra_program,
                            )?,
                            contract_class_version: declare_v2
                                .contract_class
                                .contract_class_version,
                            entry_points_by_type: declare_v2.contract_class.entry_points_by_type,
                            abi: declare_v2.contract_class.abi,
                        },
                        compiled_class_hash: declare_v2.compiled_class_hash,
                        sender_address: declare_v2.sender_address,
                        nonce: declare_v2.nonce,
                        max_fee: declare_v2.max_fee,
                        version: declare_v2.version,
                        signature: declare_v2.signature,
                    }))
                }
            },
        }
    }
}

pub type ConversionError = StorageSerdeError;

fn compress<T: StorageSerde>(value: &T) -> Result<String, ConversionError> {
    Ok(base64::encode(serialize_and_compress(value)?))
}
