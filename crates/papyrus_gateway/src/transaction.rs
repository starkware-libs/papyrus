use papyrus_storage::body::events::ThinTransactionOutput;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus};
use starknet_api::core::{
    ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, Nonce,
};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Calldata, DeclareTransactionOutput, DeployAccountTransaction, DeployAccountTransactionOutput,
    DeployTransaction, DeployTransactionOutput, Fee, InvokeTransactionOutput, L1HandlerTransaction,
    L1HandlerTransactionOutput, TransactionHash, TransactionSignature, TransactionVersion,
};

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
    Full(Vec<TransactionWithType>),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct DeclareTransactionV0V1 {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
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
    pub transaction_hash: TransactionHash,
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
            transaction_hash: tx.transaction_hash,
            signature: tx.signature,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum DeclareTransaction {
    Version0(DeclareTransactionV0V1),
    Version1(DeclareTransactionV0V1),
    Version2(DeclareTransactionV2),
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

impl From<starknet_api::transaction::InvokeTransactionV0> for InvokeTransactionV0 {
    fn from(tx: starknet_api::transaction::InvokeTransactionV0) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
            max_fee: tx.max_fee,
            version: tx_v0(),
            signature: tx.signature,
            nonce: tx.nonce,
            contract_address: tx.sender_address,
            entry_point_selector: tx.entry_point_selector,
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

impl From<starknet_api::transaction::InvokeTransactionV1> for InvokeTransactionV1 {
    fn from(tx: starknet_api::transaction::InvokeTransactionV1) -> Self {
        Self {
            transaction_hash: tx.transaction_hash,
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

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(untagged)]
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
            Transaction::Declare(DeclareTransaction::Version0(tx)) => tx.transaction_hash,
            Transaction::Declare(DeclareTransaction::Version1(tx)) => tx.transaction_hash,
            Transaction::Declare(DeclareTransaction::Version2(tx)) => tx.transaction_hash,
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
            starknet_api::transaction::Transaction::Declare(declare_tx) => match declare_tx {
                starknet_api::transaction::DeclareTransaction::V0(tx) => {
                    Self::Declare(DeclareTransaction::Version0(DeclareTransactionV0V1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: tx_v0(),
                        transaction_hash: tx.transaction_hash,
                        signature: tx.signature,
                    }))
                }
                starknet_api::transaction::DeclareTransaction::V1(tx) => {
                    Self::Declare(DeclareTransaction::Version0(DeclareTransactionV0V1 {
                        class_hash: tx.class_hash,
                        sender_address: tx.sender_address,
                        nonce: tx.nonce,
                        max_fee: tx.max_fee,
                        version: tx_v1(),
                        transaction_hash: tx.transaction_hash,
                        signature: tx.signature,
                    }))
                }
                starknet_api::transaction::DeclareTransaction::V2(tx) => {
                    Self::Declare(DeclareTransaction::Version2(tx.into()))
                }
            },
            starknet_api::transaction::Transaction::Deploy(deploy_tx) => {
                Transaction::Deploy(deploy_tx)
            }
            starknet_api::transaction::Transaction::DeployAccount(deploy_tx) => {
                Transaction::DeployAccount(deploy_tx)
            }
            starknet_api::transaction::Transaction::Invoke(invoke_tx) => match invoke_tx {
                starknet_api::transaction::InvokeTransaction::V0(tx) => {
                    Self::Invoke(InvokeTransaction::Version0(tx.into()))
                }
                starknet_api::transaction::InvokeTransaction::V1(tx) => {
                    Self::Invoke(InvokeTransaction::Version1(tx.into()))
                }
            },
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

impl From<TransactionStatus> for BlockStatus {
    fn from(status: TransactionStatus) -> Self {
        match status {
            TransactionStatus::AcceptedOnL1 => BlockStatus::AcceptedOnL1,
            TransactionStatus::AcceptedOnL2 => BlockStatus::AcceptedOnL2,
            TransactionStatus::Pending => BlockStatus::Pending,
            TransactionStatus::Rejected => BlockStatus::Rejected,
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
