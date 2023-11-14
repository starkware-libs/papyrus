#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

use jsonrpsee::types::ErrorObjectOwned;
use lazy_static::lazy_static;
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
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    DeployTransaction,
    Fee,
    L1HandlerTransaction,
    MessageToL1,
    TransactionExecutionStatus,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
};
use starknet_client::writer::objects::transaction as client_transaction;

use super::error::BLOCK_NOT_FOUND;
use crate::internal_server_error;

lazy_static! {
    static ref TX_V0: TransactionVersion = TransactionVersion::ZERO;
    static ref TX_V1: TransactionVersion = TransactionVersion::ONE;
    static ref TX_V2: TransactionVersion = TransactionVersion::TWO;
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
            version: *TX_V2,
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
    if v0v1.version == *TX_V0 {
        Ok(v0v1)
    } else {
        Err(serde::de::Error::custom("Invalid version value"))
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployAccountTransactionV1 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub version: TransactionVersion,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum DeployAccountTransaction {
    Version1(DeployAccountTransactionV1),
}

// TODO(shahak, 01/11/2023): Add test that v3 transactions cause error.
impl TryFrom<starknet_api::transaction::DeployAccountTransaction> for DeployAccountTransaction {
    type Error = ErrorObjectOwned;

    fn try_from(
        tx: starknet_api::transaction::DeployAccountTransaction,
    ) -> Result<Self, Self::Error> {
        match tx {
            starknet_api::transaction::DeployAccountTransaction::V1(
                starknet_api::transaction::DeployAccountTransactionV1 {
                    max_fee,
                    signature,
                    nonce,
                    class_hash,
                    contract_address_salt,
                    constructor_calldata,
                },
            ) => Ok(Self::Version1(DeployAccountTransactionV1 {
                max_fee,
                signature,
                nonce,
                class_hash,
                contract_address_salt,
                constructor_calldata,
                version: *TX_V1,
            })),
            starknet_api::transaction::DeployAccountTransaction::V3(_) => {
                Err(internal_server_error(
                    "The requested transaction is a deploy account of version 3, which is not \
                     supported on v0.5.1.",
                ))
            }
        }
    }
}

impl From<DeployAccountTransaction> for client_transaction::DeployAccountTransaction {
    fn from(tx: DeployAccountTransaction) -> Self {
        match tx {
            DeployAccountTransaction::Version1(deploy_account_tx) => {
                Self::DeployAccountV1(client_transaction::DeployAccountV1Transaction {
                    contract_address_salt: deploy_account_tx.contract_address_salt,
                    class_hash: deploy_account_tx.class_hash,
                    constructor_calldata: deploy_account_tx.constructor_calldata,
                    nonce: deploy_account_tx.nonce,
                    max_fee: deploy_account_tx.max_fee,
                    signature: deploy_account_tx.signature,
                    version: deploy_account_tx.version,
                    r#type: client_transaction::DeployAccountType::default(),
                })
            }
        }
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

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV1 {
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
}

impl From<InvokeTransactionV1> for client_transaction::InvokeTransaction {
    fn from(tx: InvokeTransactionV1) -> Self {
        Self::InvokeV1(client_transaction::InvokeV1Transaction {
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature,
            nonce: tx.nonce,
            sender_address: tx.sender_address,
            calldata: tx.calldata,
            r#type: client_transaction::InvokeType::default(),
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum InvokeTransaction {
    Version0(InvokeTransactionV0),
    Version1(InvokeTransactionV1),
}

// TODO(shahak, 01/11/2023): Add test that v3 transactions cause error.
impl TryFrom<starknet_api::transaction::InvokeTransaction> for InvokeTransaction {
    type Error = ErrorObjectOwned;

    fn try_from(tx: starknet_api::transaction::InvokeTransaction) -> Result<Self, Self::Error> {
        match tx {
            starknet_api::transaction::InvokeTransaction::V0(
                starknet_api::transaction::InvokeTransactionV0 {
                    max_fee,
                    signature,
                    contract_address,
                    entry_point_selector,
                    calldata,
                },
            ) => Ok(Self::Version0(InvokeTransactionV0 {
                max_fee,
                version: *TX_V0,
                signature,
                contract_address,
                entry_point_selector,
                calldata,
            })),
            starknet_api::transaction::InvokeTransaction::V1(
                starknet_api::transaction::InvokeTransactionV1 {
                    max_fee,
                    signature,
                    nonce,
                    sender_address,
                    calldata,
                },
            ) => Ok(Self::Version1(InvokeTransactionV1 {
                max_fee,
                version: *TX_V1,
                signature,
                nonce,
                sender_address,
                calldata,
            })),
            starknet_api::transaction::InvokeTransaction::V3(_) => Err(internal_server_error(
                "The requested transaction is an invoke of version 3, which is not supported on \
                 v0.5.1.",
            )),
        }
    }
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

// TODO(shahak, 01/11/2023): Add test that v3 transactions cause error.
impl TryFrom<starknet_api::transaction::Transaction> for Transaction {
    type Error = ErrorObjectOwned;

    fn try_from(tx: starknet_api::transaction::Transaction) -> Result<Self, Self::Error> {
        match tx {
            starknet_api::transaction::Transaction::Declare(declare_tx) => match declare_tx {
                // TODO(shahak, 01/11/2023): impl TryFrom for declare separately.
                starknet_api::transaction::DeclareTransaction::V0(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version0(DeclareTransactionV0V1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: *TX_V0,
                        signature: tx.signature,
                    })))
                }
                starknet_api::transaction::DeclareTransaction::V1(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version1(DeclareTransactionV0V1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: *TX_V1,
                        signature: tx.signature,
                    })))
                }
                starknet_api::transaction::DeclareTransaction::V2(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version2(tx.into())))
                }
                starknet_api::transaction::DeclareTransaction::V3(_) => Err(internal_server_error(
                    "The requested transaction is a declare of version 3, which is not supported \
                     on v0.5.1.",
                )),
            },
            starknet_api::transaction::Transaction::Deploy(deploy_tx) => {
                Ok(Transaction::Deploy(deploy_tx))
            }
            starknet_api::transaction::Transaction::DeployAccount(deploy_account_tx) => {
                match deploy_account_tx {
                    starknet_api::transaction::DeployAccountTransaction::V3(_) => {
                        Err(internal_server_error(
                            "The requested transaction is a deploy account of version 3, which is \
                             not supported on v0.5.1.",
                        ))
                    }
                    _ => Ok(Self::DeployAccount(deploy_account_tx.try_into()?)),
                }
            }
            starknet_api::transaction::Transaction::Invoke(invoke_tx) => match invoke_tx {
                starknet_api::transaction::InvokeTransaction::V3(_) => Err(internal_server_error(
                    "The requested transaction is a invoke of version 3, which is not supported \
                     on v0.5.1.",
                )),
                _ => Ok(Self::Invoke(invoke_tx.try_into()?)),
            },
            starknet_api::transaction::Transaction::L1Handler(l1_handler_tx) => {
                Ok(Transaction::L1Handler(l1_handler_tx))
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default)]
pub struct TransactionStatus {
    pub finality_status: TransactionFinalityStatus,
    pub execution_status: TransactionExecutionStatus,
}

/// Transaction Finality status on starknet.
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionFinalityStatus {
    /// The transaction passed the validation and entered an actual created block.
    #[serde(rename = "ACCEPTED_ON_L2")]
    #[default]
    AcceptedOnL2,
    /// The transaction was accepted on-chain.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
}

/// Transaction Finality status on starknet for transactions in the pending block.
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum PendingTransactionFinalityStatus {
    #[serde(rename = "ACCEPTED_ON_L2")]
    #[default]
    AcceptedOnL2,
}

impl From<BlockStatus> for TransactionFinalityStatus {
    fn from(status: BlockStatus) -> Self {
        match status {
            BlockStatus::AcceptedOnL1 => TransactionFinalityStatus::AcceptedOnL1,
            BlockStatus::AcceptedOnL2 => TransactionFinalityStatus::AcceptedOnL2,
            BlockStatus::Pending => TransactionFinalityStatus::AcceptedOnL2, /* for backward compatibility pending transactions are considered accepted on L2 */
            // we convert the block status to transaction status only in the creation of
            // TransactionReceiptWithStatus before that we verify that the block is not
            // rejected so this conversion should never happen
            BlockStatus::Rejected => unreachable!("Rejected blocks are not returned by the API"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GeneralTransactionReceipt {
    TransactionReceipt(TransactionReceipt),
    PendingTransactionReceipt(PendingTransactionReceipt),
}

impl GeneralTransactionReceipt {
    pub fn transaction_status(&self) -> TransactionStatus {
        match self {
            GeneralTransactionReceipt::TransactionReceipt(receipt) => TransactionStatus {
                execution_status: receipt.output.execution_status().clone(),
                finality_status: receipt.finality_status,
            },
            GeneralTransactionReceipt::PendingTransactionReceipt(receipt) => TransactionStatus {
                execution_status: receipt.output.execution_status().clone(),
                finality_status: TransactionFinalityStatus::AcceptedOnL2,
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct TransactionReceipt {
    pub finality_status: TransactionFinalityStatus,
    pub transaction_hash: TransactionHash,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct PendingTransactionReceipt {
    pub finality_status: PendingTransactionFinalityStatus,
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub output: PendingTransactionOutput,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
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

/// A declare transaction output.
// Note: execution_resources is not included in the output because it is not used in this version.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclareTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<starknet_api::transaction::Event>,
    pub execution_status: TransactionExecutionStatus,
}

/// A deploy-account transaction output.
// Note: execution_resources is not included in the output because it is not used in this version.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeployAccountTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<starknet_api::transaction::Event>,
    pub contract_address: ContractAddress,
    pub execution_status: TransactionExecutionStatus,
}

/// A deploy transaction output.
// Note: execution_resources is not included in the output because it is not used in this version.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeployTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<starknet_api::transaction::Event>,
    pub contract_address: ContractAddress,
    pub execution_status: TransactionExecutionStatus,
}

/// An invoke transaction output.
// Note: execution_resources is not included in the output because it is not used in this version.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct InvokeTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<starknet_api::transaction::Event>,
    pub execution_status: TransactionExecutionStatus,
}

/// An L1 handler transaction output.
// Note: execution_resources is not included in the output because it is not used in this version.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct L1HandlerTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<starknet_api::transaction::Event>,
    pub execution_status: TransactionExecutionStatus,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type")]
// Applying deny_unknown_fields on the inner type instead of on PendingTransactionReceipt because
// of a bug that makes deny_unknown_fields not work well with flatten:
// https://github.com/serde-rs/serde/issues/1358
#[serde(deny_unknown_fields)]
pub enum PendingTransactionOutput {
    #[serde(rename = "DECLARE")]
    Declare(DeclareTransactionOutput),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransactionOutput),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransactionOutput),
    #[serde(rename = "L1_HANDLER")]
    L1Handler(L1HandlerTransactionOutput),
}

impl PendingTransactionOutput {
    pub fn execution_status(&self) -> &TransactionExecutionStatus {
        match self {
            PendingTransactionOutput::Declare(tx_output) => &tx_output.execution_status,
            PendingTransactionOutput::DeployAccount(tx_output) => &tx_output.execution_status,
            PendingTransactionOutput::Invoke(tx_output) => &tx_output.execution_status,
            PendingTransactionOutput::L1Handler(tx_output) => &tx_output.execution_status,
        }
    }
}

impl TransactionOutput {
    pub fn execution_status(&self) -> &TransactionExecutionStatus {
        match self {
            TransactionOutput::Declare(tx_output) => &tx_output.execution_status,
            TransactionOutput::Deploy(tx_output) => &tx_output.execution_status,
            TransactionOutput::DeployAccount(tx_output) => &tx_output.execution_status,
            TransactionOutput::Invoke(tx_output) => &tx_output.execution_status,
            TransactionOutput::L1Handler(tx_output) => &tx_output.execution_status,
        }
    }

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
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn from(tx_output: starknet_api::transaction::TransactionOutput) -> Self {
        match tx_output {
            starknet_api::transaction::TransactionOutput::Declare(declare_tx_output) => {
                TransactionOutput::Declare(DeclareTransactionOutput {
                    actual_fee: declare_tx_output.actual_fee,
                    messages_sent: declare_tx_output.messages_sent,
                    events: declare_tx_output.events,
                    execution_status: declare_tx_output.execution_status,
                })
            }
            starknet_api::transaction::TransactionOutput::Deploy(deploy_tx_output) => {
                TransactionOutput::Deploy(DeployTransactionOutput {
                    actual_fee: deploy_tx_output.actual_fee,
                    messages_sent: deploy_tx_output.messages_sent,
                    events: deploy_tx_output.events,
                    contract_address: deploy_tx_output.contract_address,
                    execution_status: deploy_tx_output.execution_status,
                })
            }
            starknet_api::transaction::TransactionOutput::DeployAccount(deploy_tx_output) => {
                TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                    actual_fee: deploy_tx_output.actual_fee,
                    messages_sent: deploy_tx_output.messages_sent,
                    events: deploy_tx_output.events,
                    contract_address: deploy_tx_output.contract_address,
                    execution_status: deploy_tx_output.execution_status,
                })
            }
            starknet_api::transaction::TransactionOutput::Invoke(invoke_tx_output) => {
                TransactionOutput::Invoke(InvokeTransactionOutput {
                    actual_fee: invoke_tx_output.actual_fee,
                    messages_sent: invoke_tx_output.messages_sent,
                    events: invoke_tx_output.events,
                    execution_status: invoke_tx_output.execution_status,
                })
            }
            starknet_api::transaction::TransactionOutput::L1Handler(l1_handler_tx_output) => {
                TransactionOutput::L1Handler(L1HandlerTransactionOutput {
                    actual_fee: l1_handler_tx_output.actual_fee,
                    messages_sent: l1_handler_tx_output.messages_sent,
                    events: l1_handler_tx_output.events,
                    execution_status: l1_handler_tx_output.execution_status,
                })
            }
        }
    }
}

impl TryFrom<TransactionOutput> for PendingTransactionOutput {
    type Error = ErrorObjectOwned;

    fn try_from(tx_output: TransactionOutput) -> Result<Self, Self::Error> {
        match tx_output {
            TransactionOutput::Declare(declare_tx_output) => {
                Ok(PendingTransactionOutput::Declare(declare_tx_output))
            }
            TransactionOutput::Deploy(_) => {
                Err(internal_server_error("Got a pending deploy transaction."))
            }
            TransactionOutput::DeployAccount(deploy_tx_output) => {
                Ok(PendingTransactionOutput::DeployAccount(deploy_tx_output))
            }
            TransactionOutput::Invoke(invoke_tx_output) => {
                Ok(PendingTransactionOutput::Invoke(invoke_tx_output))
            }
            TransactionOutput::L1Handler(l1_handler_tx_output) => {
                Ok(PendingTransactionOutput::L1Handler(l1_handler_tx_output))
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event {
    // Can't have a different struct for pending events because then that struct will need to have
    // deny_unknown_fields. And there's a bug in serde that forbids having deny_unknown_fields with
    // flatten: https://github.com/serde-rs/serde/issues/1701
    // TODO(shahak): Create a PendingEvent struct when the serde bug is solved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub block_hash: Option<BlockHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub block_number: Option<BlockNumber>,
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
        .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;

    transactions.into_iter().map(Transaction::try_from).collect()
}

pub fn get_block_tx_hashes_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<TransactionHash>, ErrorObjectOwned> {
    let transaction_hashes = txn
        .get_block_transaction_hashes(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;

    Ok(transaction_hashes)
}
