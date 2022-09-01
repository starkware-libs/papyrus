use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockStatus, CallData, ContractAddress, DeclareTransaction, DeployTransaction,
    EntryPointSelector, Fee, Nonce, TransactionHash, TransactionReceipt, TransactionSignature,
    TransactionVersion,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
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
    pub calldata: CallData,
}

impl From<starknet_api::InvokeTransaction> for InvokeTransactionV0 {
    fn from(tx: starknet_api::InvokeTransaction) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature,
            nonce: tx.nonce,
            contract_address: tx.contract_address,
            entry_point_selector: tx.entry_point_selector.unwrap_or_default(),
            calldata: tx.call_data,
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
    pub calldata: CallData,
}

impl From<starknet_api::InvokeTransaction> for InvokeTransactionV1 {
    fn from(tx: starknet_api::InvokeTransaction) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx.version,
            signature: tx.signature,
            nonce: tx.nonce,
            sender_address: tx.contract_address,
            calldata: tx.call_data,
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
pub enum Transaction {
    Declare(DeclareTransaction),
    Deploy(DeployTransaction),
    Invoke(InvokeTransaction),
}

impl Transaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            Transaction::Declare(tx) => tx.transaction_hash,
            Transaction::Deploy(tx) => tx.transaction_hash,
            Transaction::Invoke(InvokeTransaction::Version0(tx)) => tx.transaction_hash,
            Transaction::Invoke(InvokeTransaction::Version1(tx)) => tx.transaction_hash,
        }
    }
}

impl From<starknet_api::Transaction> for Transaction {
    fn from(tx: starknet_api::Transaction) -> Self {
        match tx {
            starknet_api::Transaction::Declare(declare_tx) => Transaction::Declare(declare_tx),
            starknet_api::Transaction::Deploy(deploy_tx) => Transaction::Deploy(deploy_tx),
            starknet_api::Transaction::Invoke(invoke_tx) => {
                if invoke_tx.entry_point_selector.is_none() {
                    Transaction::Invoke(InvokeTransaction::Version1(invoke_tx.into()))
                } else {
                    Transaction::Invoke(InvokeTransaction::Version0(invoke_tx.into()))
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "INVOKE", serialize = "INVOKE"))]
    Invoke,
}
impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::Invoke
    }
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
            Transaction::Invoke(_) => {
                TransactionWithType { r#type: TransactionType::Invoke, transaction }
            }
        }
    }
}

impl From<starknet_api::Transaction> for TransactionWithType {
    fn from(transaction: starknet_api::Transaction) -> Self {
        Self::from(Transaction::from(transaction))
    }
}

/// A transaction status in StarkNet.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionStatus {
    /// The transaction passed the validation and entered the pending block.
    #[serde(rename = "PENDING")]
    Pending,
    /// The transaction passed the validation and entered an actual created block.
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    /// The transaction was accepted on-chain.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    /// The transaction failed validation.
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::AcceptedOnL2
    }
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
