use papyrus_storage::body::events::ThinTransactionOutput;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus};
use starknet_api::core::{ContractAddress, EntryPointSelector, Nonce};
use starknet_api::transaction::{
    Calldata, DeclareTransaction, DeclareTransactionOutput, DeployAccountTransaction,
    DeployAccountTransactionOutput, DeployTransaction, DeployTransactionOutput, Fee,
    InvokeTransactionOutput, L1HandlerTransaction, L1HandlerTransactionOutput, TransactionHash,
    TransactionSignature, TransactionVersion,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Transactions {
    Hashes(Vec<TransactionHash>),
    Full(Vec<TransactionWithType>),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV0 {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl From<starknet_api::transaction::InvokeTransaction> for InvokeTransactionV0 {
    fn from(tx: starknet_api::transaction::InvokeTransaction) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature,
            nonce: tx.nonce,
            contract_address: tx.sender_address,
            entry_point_selector: tx.entry_point_selector.unwrap_or_default(),
            calldata: tx.calldata,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV1 {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
}

impl From<starknet_api::transaction::InvokeTransaction> for InvokeTransactionV1 {
    fn from(tx: starknet_api::transaction::InvokeTransaction) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
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
#[serde(untagged)]
// Note: When deserializing an untagged enum, no variant can be a prefix of variants to follow.
pub enum Transaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Deploy(DeployTransaction),
    Invoke(InvokeTransaction),
    L1Handler(L1HandlerTransaction),
}

impl Transaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            Transaction::Declare(tx) => tx.transaction_hash,
            Transaction::Deploy(tx) => tx.transaction_hash,
            Transaction::DeployAccount(tx) => tx.transaction_hash,
            Transaction::Invoke(InvokeTransaction::Version0(tx)) => tx.transaction_hash,
            Transaction::Invoke(InvokeTransaction::Version1(tx)) => tx.transaction_hash,
            Transaction::L1Handler(tx) => tx.transaction_hash,
        }
    }
}

impl From<starknet_api::transaction::Transaction> for Transaction {
    fn from(tx: starknet_api::transaction::Transaction) -> Self {
        match tx {
            starknet_api::transaction::Transaction::Declare(declare_tx) => {
                Transaction::Declare(declare_tx)
            }
            starknet_api::transaction::Transaction::Deploy(deploy_tx) => {
                Transaction::Deploy(deploy_tx)
            }
            starknet_api::transaction::Transaction::DeployAccount(deploy_tx) => {
                Transaction::DeployAccount(deploy_tx)
            }
            starknet_api::transaction::Transaction::Invoke(invoke_tx) => {
                if invoke_tx.entry_point_selector.is_none() {
                    Transaction::Invoke(InvokeTransaction::Version1(invoke_tx.into()))
                } else {
                    Transaction::Invoke(InvokeTransaction::Version0(invoke_tx.into()))
                }
            }
            starknet_api::transaction::Transaction::L1Handler(l1_handler_tx) => {
                Transaction::L1Handler(l1_handler_tx)
            }
        }
    }
}

#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "DEPLOY_ACCOUNT", serialize = "DEPLOY_ACCOUNT"))]
    DeployAccount,
    #[serde(rename(deserialize = "INVOKE", serialize = "INVOKE"))]
    #[default]
    Invoke,
    #[serde(rename(deserialize = "L1_HANDLER", serialize = "L1_HANDLER"))]
    L1Handler,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionWithType {
    pub r#type: TransactionType,
    #[serde(flatten)]
    pub transaction: Transaction,
}

impl From<Transaction> for TransactionWithType {
    fn from(transaction: Transaction) -> Self {
        match transaction {
            Transaction::Declare(_) => {
                TransactionWithType { r#type: TransactionType::Declare, transaction }
            }
            Transaction::Deploy(_) => {
                TransactionWithType { r#type: TransactionType::Deploy, transaction }
            }
            Transaction::DeployAccount(_) => {
                TransactionWithType { r#type: TransactionType::DeployAccount, transaction }
            }
            Transaction::Invoke(_) => {
                TransactionWithType { r#type: TransactionType::Invoke, transaction }
            }
            Transaction::L1Handler(_) => {
                TransactionWithType { r#type: TransactionType::L1Handler, transaction }
            }
        }
    }
}

impl From<starknet_api::transaction::Transaction> for TransactionWithType {
    fn from(transaction: starknet_api::transaction::Transaction) -> Self {
        Self::from(Transaction::from(transaction))
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
            BlockStatus::Rejected => TransactionStatus::Rejected,
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
#[serde(untagged)]
pub enum TransactionReceipt {
    Deploy(DeployTransactionReceipt),
    Common(CommonTransactionReceipt),
}

impl TransactionReceipt {
    pub fn from_transaction_output(
        output: TransactionOutput,
        transaction: &starknet_api::transaction::Transaction,
        block_hash: BlockHash,
        block_number: BlockNumber,
    ) -> Self {
        let common = CommonTransactionReceipt {
            transaction_hash: transaction.transaction_hash(),
            r#type: output.r#type(),
            block_hash,
            block_number,
            output,
        };

        match transaction {
            starknet_api::transaction::Transaction::DeployAccount(tx) => {
                Self::Deploy(DeployTransactionReceipt {
                    common,
                    contract_address: tx.contract_address,
                })
            }
            starknet_api::transaction::Transaction::Deploy(tx) => {
                Self::Deploy(DeployTransactionReceipt {
                    common,
                    contract_address: tx.contract_address,
                })
            }
            _ => Self::Common(common),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransactionReceipt {
    #[serde(flatten)]
    pub common: CommonTransactionReceipt,
    pub contract_address: ContractAddress,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CommonTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub r#type: TransactionType,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum TransactionOutput {
    Declare(DeclareTransactionOutput),
    Deploy(DeployTransactionOutput),
    DeployAccount(DeployAccountTransactionOutput),
    Invoke(InvokeTransactionOutput),
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
                })
            }
            ThinTransactionOutput::Deploy(thin_deploy) => {
                TransactionOutput::Deploy(DeployTransactionOutput {
                    actual_fee: thin_deploy.actual_fee,
                    messages_sent: thin_deploy.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::DeployAccount(thin_deploy) => {
                TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                    actual_fee: thin_deploy.actual_fee,
                    messages_sent: thin_deploy.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::Invoke(thin_invoke) => {
                TransactionOutput::Invoke(InvokeTransactionOutput {
                    actual_fee: thin_invoke.actual_fee,
                    messages_sent: thin_invoke.messages_sent,
                    events,
                })
            }
            ThinTransactionOutput::L1Handler(thin_l1handler) => {
                TransactionOutput::L1Handler(L1HandlerTransactionOutput {
                    actual_fee: thin_l1handler.actual_fee,
                    messages_sent: thin_l1handler.messages_sent,
                    events,
                })
            }
        }
    }

    pub fn r#type(&self) -> TransactionType {
        match self {
            TransactionOutput::Declare(_) => TransactionType::Declare,
            TransactionOutput::Deploy(_) => TransactionType::Deploy,
            TransactionOutput::DeployAccount(_) => TransactionType::DeployAccount,
            TransactionOutput::Invoke(_) => TransactionType::Invoke,
            TransactionOutput::L1Handler(_) => TransactionType::L1Handler,
        }
    }
}

impl From<starknet_api::transaction::TransactionOutput> for TransactionOutput {
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
