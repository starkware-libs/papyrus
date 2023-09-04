#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

use jsonrpsee::types::ErrorObjectOwned;
use papyrus_storage::body::events::ThinTransactionOutput;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::StorageTxn;
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    Nonce,
};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Calldata,
    DeclareTransactionOutput,
    DeployAccountTransaction,
    DeployAccountTransactionOutput,
    DeployTransaction,
    DeployTransactionOutput,
    Fee,
    InvokeTransactionOutput,
    L1HandlerTransaction,
    L1HandlerTransactionOutput,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
};

use crate::internal_server_error;
use crate::v0_3_0::error::JsonRpcError;

// TODO(yair): Make these functions regular consts.
fn tx_v0() -> TransactionVersion {
    TransactionVersion(StarkFelt::try_from("0x0").expect("Unable to convert 0x0 to StarkFelt."))
}
fn tx_v1() -> TransactionVersion {
    TransactionVersion(StarkFelt::try_from("0x1").expect("Unable to convert 0x1 to StarkFelt."))
}
fn tx_v2() -> TransactionVersion {
    TransactionVersion(StarkFelt::try_from("0x2").expect("Unable to convert 0x2 to StarkFelt."))
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TransactionWithHash>),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct DeclareTransactionV0V1 {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
}
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionV2 {
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
}

impl From<starknet_api::transaction::DeclareTransactionV2> for DeclareTransactionV2 {
    fn from(tx: starknet_api::transaction::DeclareTransactionV2) -> Self {
        Self {
            class_hash: tx.class_hash,
            compiled_class_hash: tx.compiled_class_hash,
            sender_address: tx.sender_address,
            nonce: tx.nonce,
            max_fee: tx.max_fee,
            version: tx_v2(),
            signature: tx.signature,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum DeclareTransaction {
    #[serde(deserialize_with = "declare_v0_deserialize")]
    Version0(DeclareTransactionV0V1),
    Version1(DeclareTransactionV0V1),
    Version2(DeclareTransactionV2),
}

fn declare_v0_deserialize<'de, D>(deserializer: D) -> Result<DeclareTransactionV0V1, D::Error>
where
    D: Deserializer<'de>,
{
    let v0v1: DeclareTransactionV0V1 = Deserialize::deserialize(deserializer)?;
    if v0v1.version == tx_v0() {
        Ok(v0v1)
    } else {
        Err(serde::de::Error::custom("Invalid version value"))
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV0 {
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl From<starknet_api::transaction::InvokeTransactionV0> for InvokeTransactionV0 {
    fn from(tx: starknet_api::transaction::InvokeTransactionV0) -> Self {
        Self {
            max_fee: tx.max_fee,
            version: tx_v0(),
            signature: tx.signature,
            contract_address: tx.contract_address,
            entry_point_selector: tx.entry_point_selector,
            calldata: tx.calldata,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV1 {
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
}

impl From<starknet_api::transaction::InvokeTransactionV1> for InvokeTransactionV1 {
    fn from(tx: starknet_api::transaction::InvokeTransactionV1) -> Self {
        Self {
            max_fee: tx.max_fee,
            version: tx_v1(),
            signature: tx.signature,
            nonce: tx.nonce,
            sender_address: tx.sender_address,
            calldata: tx.calldata,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum InvokeTransaction {
    Version0(InvokeTransactionV0),
    Version1(InvokeTransactionV1),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionWithHash {
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub transaction: Transaction,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(tag = "type")]
pub enum Transaction {
    #[serde(rename = "DECLARE")]
    Declare(DeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransaction),
    #[serde(rename = "DEPLOY")]
    Deploy(DeployTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransaction),
    #[serde(rename = "L1_HANDLER")]
    L1Handler(L1HandlerTransaction),
}

impl TryFrom<starknet_api::transaction::Transaction> for Transaction {
    type Error = ErrorObjectOwned;

    fn try_from(tx: starknet_api::transaction::Transaction) -> Result<Self, Self::Error> {
        match tx {
            starknet_api::transaction::Transaction::Declare(declare_tx) => match declare_tx {
                starknet_api::transaction::DeclareTransaction::V0(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version0(DeclareTransactionV0V1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: tx_v0(),
                        signature: tx.signature,
                    })))
                }
                starknet_api::transaction::DeclareTransaction::V1(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version1(DeclareTransactionV0V1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: tx_v1(),
                        signature: tx.signature,
                    })))
                }
                starknet_api::transaction::DeclareTransaction::V2(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version2(tx.into())))
                }
                starknet_api::transaction::DeclareTransaction::V3(_) => {
                    Err(internal_server_error("Version 3 transactions are not supported on v0.3.0"))
                }
            },
            starknet_api::transaction::Transaction::Deploy(deploy_tx) => {
                Ok(Transaction::Deploy(deploy_tx))
            }
            starknet_api::transaction::Transaction::DeployAccount(deploy_tx) => {
                Ok(Transaction::DeployAccount(deploy_tx))
            }
            starknet_api::transaction::Transaction::Invoke(invoke_tx) => match invoke_tx {
                starknet_api::transaction::InvokeTransaction::V0(tx) => {
                    Ok(Self::Invoke(InvokeTransaction::Version0(tx.into())))
                }
                starknet_api::transaction::InvokeTransaction::V1(tx) => {
                    Ok(Self::Invoke(InvokeTransaction::Version1(tx.into())))
                }
            },
            starknet_api::transaction::Transaction::L1Handler(l1_handler_tx) => {
                Ok(Transaction::L1Handler(l1_handler_tx))
            }
        }
    }
}

/// A transaction status in StarkNet.
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionStatus {
    /// The transaction passed the validation and entered the pending block.
    #[serde(rename = "PENDING")]
    Pending,
    /// The transaction passed the validation and entered an actual created block.
    #[serde(rename = "ACCEPTED_ON_L2")]
    #[default]
    AcceptedOnL2,
    /// The transaction was accepted on-chain.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    /// The transaction failed validation.
    #[serde(rename = "REJECTED")]
    Rejected,
}

impl From<BlockStatus> for TransactionStatus {
    fn from(status: BlockStatus) -> Self {
        match status {
            BlockStatus::AcceptedOnL1 => TransactionStatus::AcceptedOnL1,
            BlockStatus::AcceptedOnL2 => TransactionStatus::AcceptedOnL2,
            BlockStatus::Pending => TransactionStatus::Pending,
            // we convert the block status to transaction status only in the creation of
            // TransactionReceiptWithStatus before that we verify that the block is not
            // rejected so this conversion should never happen
            BlockStatus::Rejected => unreachable!("Rejected blocks are not returned by the API"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceiptWithStatus {
    pub status: TransactionStatus,
    #[serde(flatten)]
    pub receipt: TransactionReceipt,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(tag = "type")]
pub enum TransactionOutput {
    #[serde(rename = "DECLARE")]
    Declare(DeclareTransactionOutput),
    #[serde(rename = "DEPLOY")]
    Deploy(DeployTransactionOutput),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransactionOutput),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransactionOutput),
    #[serde(rename = "L1_HANDLER")]
    L1Handler(L1HandlerTransactionOutput),
}

impl TransactionOutput {
    pub fn from_thin_transaction_output(
        thin_tx_output: ThinTransactionOutput,
        events: Vec<starknet_api::transaction::Event>,
    ) -> Self {
        match thin_tx_output {
            ThinTransactionOutput::Declare(thin_declare) => {
                TransactionOutput::Declare(DeclareTransactionOutput {
                    actual_fee: thin_declare.actual_fee,
                    messages_sent: thin_declare.messages_sent,
                    events,
                    execution_status: thin_declare.execution_status,
                })
            }
            ThinTransactionOutput::Deploy(thin_deploy) => {
                TransactionOutput::Deploy(DeployTransactionOutput {
                    actual_fee: thin_deploy.actual_fee,
                    messages_sent: thin_deploy.messages_sent,
                    events,
                    contract_address: thin_deploy.contract_address,
                    execution_status: thin_deploy.execution_status,
                })
            }
            ThinTransactionOutput::DeployAccount(thin_deploy) => {
                TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                    actual_fee: thin_deploy.actual_fee,
                    messages_sent: thin_deploy.messages_sent,
                    events,
                    contract_address: thin_deploy.contract_address,
                    execution_status: thin_deploy.execution_status,
                })
            }
            ThinTransactionOutput::Invoke(thin_invoke) => {
                TransactionOutput::Invoke(InvokeTransactionOutput {
                    actual_fee: thin_invoke.actual_fee,
                    messages_sent: thin_invoke.messages_sent,
                    events,
                    execution_status: thin_invoke.execution_status,
                })
            }
            ThinTransactionOutput::L1Handler(thin_l1handler) => {
                TransactionOutput::L1Handler(L1HandlerTransactionOutput {
                    actual_fee: thin_l1handler.actual_fee,
                    messages_sent: thin_l1handler.messages_sent,
                    events,
                    execution_status: thin_l1handler.execution_status,
                })
            }
        }
    }
}

impl From<starknet_api::transaction::TransactionOutput> for TransactionOutput {
    #[cfg_attr(coverage_nightly, no_coverage)]
    fn from(tx_output: starknet_api::transaction::TransactionOutput) -> Self {
        match tx_output {
            starknet_api::transaction::TransactionOutput::Declare(declare_tx_output) => {
                TransactionOutput::Declare(declare_tx_output)
            }
            starknet_api::transaction::TransactionOutput::Deploy(deploy_tx_output) => {
                TransactionOutput::Deploy(deploy_tx_output)
            }
            starknet_api::transaction::TransactionOutput::DeployAccount(deploy_tx_output) => {
                TransactionOutput::DeployAccount(deploy_tx_output)
            }
            starknet_api::transaction::TransactionOutput::Invoke(invoke_tx_output) => {
                TransactionOutput::Invoke(invoke_tx_output)
            }
            starknet_api::transaction::TransactionOutput::L1Handler(l1_handler_tx_output) => {
                TransactionOutput::L1Handler(l1_handler_tx_output)
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub event: starknet_api::transaction::Event,
}

pub fn get_block_txs_by_number<
    Mode: TransactionKind,
    Transaction: TryFrom<starknet_api::transaction::Transaction, Error = ErrorObjectOwned>,
>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<Transaction>, ErrorObjectOwned> {
    let transactions = txn
        .get_block_transactions(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    transactions.into_iter().map(Transaction::try_from).collect()
}

pub fn get_block_tx_hashes_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<TransactionHash>, ErrorObjectOwned> {
    let transaction_hashes = txn
        .get_block_transaction_hashes(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(transaction_hashes)
}
