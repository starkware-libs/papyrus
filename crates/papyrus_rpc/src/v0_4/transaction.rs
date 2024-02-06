#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

use std::sync::Arc;

use jsonrpsee::types::ErrorObjectOwned;
use papyrus_storage::body::events::ThinTransactionOutput;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::StorageTxn;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    DeployTransaction,
    Fee,
    L1HandlerTransaction,
    MessageToL1,
    Resource,
    TransactionExecutionStatus,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
};
use starknet_client::writer::objects::transaction as client_transaction;
use starknet_types_core::felt::Felt;

use super::error::BLOCK_NOT_FOUND;
use crate::internal_server_error;

#[derive(
    Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord,
)]
pub enum TransactionVersion0 {
    #[serde(rename = "0x0")]
    #[default]
    Version0,
}

#[derive(
    Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord,
)]
pub enum TransactionVersion1 {
    #[serde(rename = "0x1")]
    #[default]
    Version1,
}

#[derive(
    Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord,
)]
pub enum TransactionVersion2 {
    #[serde(rename = "0x2")]
    #[default]
    Version2,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TransactionWithHash>),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct DeclareTransactionV0 {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion0,
    pub signature: TransactionSignature,
}
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct DeclareTransactionV1 {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion1,
    pub signature: TransactionSignature,
}
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionV2 {
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion2,
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
            version: TransactionVersion2::default(),
            signature: tx.signature,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum DeclareTransaction {
    Version0(DeclareTransactionV0),
    Version1(DeclareTransactionV1),
    Version2(DeclareTransactionV2),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployAccountTransactionV1 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub version: TransactionVersion1,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum DeployAccountTransaction {
    Version1(DeployAccountTransactionV1),
}

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
                version: TransactionVersion1::default(),
            })),
            starknet_api::transaction::DeployAccountTransaction::V3(
                starknet_api::transaction::DeployAccountTransactionV3 {
                    resource_bounds,
                    signature,
                    nonce,
                    class_hash,
                    contract_address_salt,
                    constructor_calldata,
                    ..
                },
            ) => {
                let l1_gas_bounds = resource_bounds
                    .0
                    .get(&Resource::L1Gas)
                    .ok_or(internal_server_error("Got a v3 transaction with no L1 gas bounds."))?;
                Ok(Self::Version1(DeployAccountTransactionV1 {
                    max_fee: Fee(
                        l1_gas_bounds.max_price_per_unit * u128::from(l1_gas_bounds.max_amount)
                    ),
                    signature,
                    nonce,
                    class_hash,
                    contract_address_salt,
                    constructor_calldata,
                    version: TransactionVersion1::default(),
                }))
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
                    version: TransactionVersion::ONE,
                    r#type: client_transaction::DeployAccountType::default(),
                })
            }
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV0 {
    pub max_fee: Fee,
    pub version: TransactionVersion0,
    pub signature: TransactionSignature,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV1 {
    pub max_fee: Fee,
    pub version: TransactionVersion1,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
}

impl From<InvokeTransactionV1> for client_transaction::InvokeTransaction {
    fn from(tx: InvokeTransactionV1) -> Self {
        Self::InvokeV1(client_transaction::InvokeV1Transaction {
            max_fee: tx.max_fee,
            version: TransactionVersion::ONE,
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
                version: TransactionVersion0::default(),
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
                version: TransactionVersion1::default(),
                signature,
                nonce,
                sender_address,
                calldata,
            })),
            starknet_api::transaction::InvokeTransaction::V3(
                starknet_api::transaction::InvokeTransactionV3 {
                    resource_bounds,
                    signature,
                    nonce,
                    sender_address,
                    calldata,
                    ..
                },
            ) => {
                let l1_gas_bounds = resource_bounds
                    .0
                    .get(&Resource::L1Gas)
                    .ok_or(internal_server_error("Got a v3 transaction with no L1 gas bounds."))?;
                Ok(Self::Version1(InvokeTransactionV1 {
                    max_fee: Fee(
                        l1_gas_bounds.max_price_per_unit * u128::from(l1_gas_bounds.max_amount)
                    ),
                    version: TransactionVersion1::default(),
                    signature,
                    nonce,
                    sender_address,
                    calldata,
                }))
            }
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

impl TryFrom<starknet_api::transaction::Transaction> for Transaction {
    type Error = ErrorObjectOwned;

    fn try_from(tx: starknet_api::transaction::Transaction) -> Result<Self, Self::Error> {
        match tx {
            starknet_api::transaction::Transaction::Declare(declare_tx) => match declare_tx {
                starknet_api::transaction::DeclareTransaction::V0(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version0(DeclareTransactionV0 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: TransactionVersion0::default(),
                        signature: tx.signature,
                    })))
                }
                starknet_api::transaction::DeclareTransaction::V1(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version1(DeclareTransactionV1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: TransactionVersion1::default(),
                        signature: tx.signature,
                    })))
                }
                starknet_api::transaction::DeclareTransaction::V2(tx) => {
                    Ok(Self::Declare(DeclareTransaction::Version2(tx.into())))
                }
                starknet_api::transaction::DeclareTransaction::V3(tx) => {
                    let l1_gas_bounds = tx.resource_bounds.0.get(&Resource::L1Gas).ok_or(
                        internal_server_error("Got a v3 transaction with no L1 gas bounds."),
                    )?;
                    Ok(Self::Declare(DeclareTransaction::Version2(DeclareTransactionV2 {
                        class_hash: tx.class_hash,
                        compiled_class_hash: tx.compiled_class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: Fee(
                            l1_gas_bounds.max_price_per_unit * u128::from(l1_gas_bounds.max_amount)
                        ),
                        version: TransactionVersion2::default(),
                        signature: tx.signature,
                    })))
                }
            },
            starknet_api::transaction::Transaction::Deploy(deploy_tx) => {
                Ok(Transaction::Deploy(deploy_tx))
            }
            starknet_api::transaction::Transaction::DeployAccount(deploy_account_tx) => {
                Ok(Self::DeployAccount(deploy_account_tx.try_into()?))
            }
            starknet_api::transaction::Transaction::Invoke(invoke_tx) => {
                Ok(Self::Invoke(invoke_tx.try_into()?))
            }
            starknet_api::transaction::Transaction::L1Handler(l1_handler_tx) => {
                Ok(Transaction::L1Handler(l1_handler_tx))
            }
        }
    }
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

impl TransactionOutput {
    #[cfg(test)]
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

/// An InvokeTransactionV1 that has the type field. This enum can be used to serialize/deserialize
/// invoke v1 transactions directly while `InvokeTransactionV1` can be serialized/deserialized only
/// from the `Transaction` enum.
/// This allows RPC methods to receive an invoke v1 transaction directly.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum TypedInvokeTransactionV1 {
    #[serde(rename = "INVOKE")]
    InvokeV1(InvokeTransactionV1),
}

impl From<TypedInvokeTransactionV1> for client_transaction::InvokeTransaction {
    fn from(tx: TypedInvokeTransactionV1) -> Self {
        let TypedInvokeTransactionV1::InvokeV1(tx) = tx;
        tx.into()
    }
}

/// A DeployAccountTransaction that has the type field. This enum can be used to
/// serialize/deserialize deploy account transactions directly while `DeployAccountTransaction` can
/// be serialized/deserialized only from the `Transaction` enum.
/// This allows RPC methods to receive a deploy account transaction directly.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum TypedDeployAccountTransaction {
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransaction),
}

impl From<TypedDeployAccountTransaction> for client_transaction::DeployAccountTransaction {
    fn from(tx: TypedDeployAccountTransaction) -> Self {
        let TypedDeployAccountTransaction::DeployAccount(tx) = tx;
        tx.into()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct MessageFromL1 {
    // TODO: fix serialization of EthAddress in SN_API to fit the spec.
    #[serde(serialize_with = "serialize_eth_address")]
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub payload: Calldata,
}

// Serialize EthAddress to a 40 character hex string with a 0x prefix.
fn serialize_eth_address<S>(eth_address: &EthAddress, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let hex_string = hex::encode(eth_address.0.as_bytes());
    let fixed_size_hex_string = format!("0x{:0<40}", hex_string);
    serializer.serialize_str(fixed_size_hex_string.as_str())
}

impl From<MessageFromL1> for L1HandlerTransaction {
    fn from(message: MessageFromL1) -> Self {
        let sender_as_felt = eth_address_to_felt(message.from_address);
        let mut calldata = vec![sender_as_felt];
        calldata.extend_from_slice(&message.payload.0);
        let calldata = Calldata(Arc::new(calldata));
        Self {
            version: TransactionVersion::ONE,
            contract_address: message.to_address,
            entry_point_selector: message.entry_point_selector,
            calldata,
            ..Default::default()
        }
    }
}

// TODO(yair): move to SN_API and implement as From.
fn eth_address_to_felt(eth_address: EthAddress) -> Felt {
    let eth_address_as_bytes = eth_address.0.to_fixed_bytes();
    let mut bytes: [u8; 32] = [0; 32];
    bytes[12..32].copy_from_slice(&eth_address_as_bytes);
    Felt::from_bytes_be(&bytes)
}
