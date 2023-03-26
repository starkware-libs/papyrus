use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, DeclareTransactionOutput, DeployAccountTransactionOutput,
    DeployTransactionOutput, EthAddress, Event, Fee, InvokeTransactionOutput,
    L1HandlerTransactionOutput, L1ToL2Payload, L2ToL1Payload, MessageToL1, TransactionHash,
    TransactionOffsetInBlock, TransactionOutput, TransactionSignature, TransactionVersion,
};

// TODO(dan): consider extracting common fields out (version, hash, type).
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
// Note: When deserializing an untagged enum, no variant can be a prefix of variants to follow.
pub enum Transaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Deploy(DeployTransaction),
    Invoke(InvokeTransaction),
    L1Handler(L1HandlerTransaction),
}

impl From<Transaction> for starknet_api::transaction::Transaction {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Declare(declare_tx) => {
                starknet_api::transaction::Transaction::Declare(declare_tx.into())
            }
            Transaction::Deploy(deploy_tx) => {
                starknet_api::transaction::Transaction::Deploy(deploy_tx.into())
            }
            Transaction::DeployAccount(deploy_acc_tx) => {
                starknet_api::transaction::Transaction::DeployAccount(deploy_acc_tx.into())
            }
            Transaction::Invoke(invoke_tx) => {
                starknet_api::transaction::Transaction::Invoke(invoke_tx.into())
            }
            Transaction::L1Handler(l1_handler_tx) => {
                starknet_api::transaction::Transaction::L1Handler(l1_handler_tx.into())
            }
        }
    }
}

impl Transaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            Transaction::Declare(tx) => tx.transaction_hash,
            Transaction::Deploy(tx) => tx.transaction_hash,
            Transaction::DeployAccount(tx) => tx.transaction_hash,
            Transaction::Invoke(tx) => tx.transaction_hash,
            Transaction::L1Handler(tx) => tx.transaction_hash,
        }
    }

    pub fn transaction_type(&self) -> TransactionType {
        match self {
            Transaction::Declare(tx) => tx.r#type,
            Transaction::Deploy(tx) => tx.r#type,
            Transaction::DeployAccount(tx) => tx.r#type,
            Transaction::Invoke(tx) => tx.r#type,
            Transaction::L1Handler(tx) => tx.r#type,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1HandlerTransaction {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    #[serde(default)]
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub r#type: TransactionType,
}

impl From<L1HandlerTransaction> for starknet_api::transaction::L1HandlerTransaction {
    fn from(l1_handler_tx: L1HandlerTransaction) -> Self {
        starknet_api::transaction::L1HandlerTransaction {
            transaction_hash: l1_handler_tx.transaction_hash,
            version: l1_handler_tx.version,
            nonce: l1_handler_tx.nonce,
            contract_address: l1_handler_tx.contract_address,
            entry_point_selector: l1_handler_tx.entry_point_selector,
            calldata: l1_handler_tx.calldata,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct DeclareTransaction {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    #[serde(default)]
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
    pub signature: TransactionSignature,
    pub r#type: TransactionType,
}

impl From<DeclareTransaction> for starknet_api::transaction::DeclareTransaction {
    fn from(declare_tx: DeclareTransaction) -> Self {
        starknet_api::transaction::DeclareTransaction {
            transaction_hash: declare_tx.transaction_hash,
            max_fee: declare_tx.max_fee,
            version: declare_tx.version,
            signature: declare_tx.signature,
            class_hash: declare_tx.class_hash,
            sender_address: declare_tx.sender_address,
            nonce: declare_tx.nonce,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct DeployTransaction {
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

impl From<DeployTransaction> for starknet_api::transaction::DeployTransaction {
    fn from(deploy_tx: DeployTransaction) -> Self {
        starknet_api::transaction::DeployTransaction {
            transaction_hash: deploy_tx.transaction_hash,
            version: deploy_tx.version,
            contract_address: deploy_tx.contract_address,
            constructor_calldata: deploy_tx.constructor_calldata,
            class_hash: deploy_tx.class_hash,
            contract_address_salt: deploy_tx.contract_address_salt,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct DeployAccountTransaction {
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

impl From<DeployAccountTransaction> for starknet_api::transaction::DeployAccountTransaction {
    fn from(deploy_tx: DeployAccountTransaction) -> Self {
        starknet_api::transaction::DeployAccountTransaction {
            transaction_hash: deploy_tx.transaction_hash,
            version: deploy_tx.version,
            contract_address: deploy_tx.contract_address,
            constructor_calldata: deploy_tx.constructor_calldata,
            class_hash: deploy_tx.class_hash,
            contract_address_salt: deploy_tx.contract_address_salt,
            max_fee: deploy_tx.max_fee,
            signature: deploy_tx.signature,
            nonce: deploy_tx.nonce,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct InvokeTransaction {
    pub calldata: Calldata,
    pub contract_address: ContractAddress,
    pub entry_point_selector: Option<EntryPointSelector>,
    pub nonce: Option<Nonce>,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

impl From<InvokeTransaction> for starknet_api::transaction::InvokeTransaction {
    fn from(invoke_tx: InvokeTransaction) -> Self {
        Self {
            transaction_hash: invoke_tx.transaction_hash,
            max_fee: invoke_tx.max_fee,
            version: invoke_tx.version,
            signature: invoke_tx.signature,
            nonce: invoke_tx.nonce.unwrap_or_default(),
            sender_address: invoke_tx.contract_address,
            entry_point_selector: invoke_tx.entry_point_selector,
            calldata: invoke_tx.calldata,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct TransactionReceipt {
    pub transaction_index: TransactionOffsetInBlock,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub l1_to_l2_consumed_message: L1ToL2Message,
    pub l2_to_l1_messages: Vec<L2ToL1Message>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub execution_resources: ExecutionResources,
    pub actual_fee: Fee,
}

impl TransactionReceipt {
    pub fn into_starknet_api_transaction_output(
        self,
        tx_type: TransactionType,
    ) -> TransactionOutput {
        let messages_sent = self.l2_to_l1_messages.into_iter().map(MessageToL1::from).collect();
        match tx_type {
            TransactionType::Declare => TransactionOutput::Declare(DeclareTransactionOutput {
                actual_fee: self.actual_fee,
                messages_sent,
                events: self.events,
            }),
            TransactionType::Deploy => TransactionOutput::Deploy(DeployTransactionOutput {
                actual_fee: self.actual_fee,
                messages_sent,
                events: self.events,
            }),
            TransactionType::DeployAccount => {
                TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                    actual_fee: self.actual_fee,
                    messages_sent,
                    events: self.events,
                })
            }
            TransactionType::InvokeFunction => TransactionOutput::Invoke(InvokeTransactionOutput {
                actual_fee: self.actual_fee,
                messages_sent,
                events: self.events,
            }),
            TransactionType::L1Handler => {
                TransactionOutput::L1Handler(L1HandlerTransactionOutput {
                    actual_fee: self.actual_fee,
                    messages_sent,
                    events: self.events,
                })
            }
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ExecutionResources {
    pub n_steps: u64,
    pub builtin_instance_counter: BuiltinInstanceCounter,
    pub n_memory_holes: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum BuiltinInstanceCounter {
    NonEmpty(HashMap<String, u64>),
    Empty(EmptyBuiltinInstanceCounter),
}

impl Default for BuiltinInstanceCounter {
    fn default() -> Self {
        BuiltinInstanceCounter::Empty(EmptyBuiltinInstanceCounter {})
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct EmptyBuiltinInstanceCounter {}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Nonce(pub StarkHash);

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

impl From<L1ToL2Message> for starknet_api::transaction::MessageToL2 {
    fn from(message: L1ToL2Message) -> Self {
        starknet_api::transaction::MessageToL2 {
            from_address: message.from_address,
            payload: message.payload,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

impl From<L2ToL1Message> for starknet_api::transaction::MessageToL1 {
    fn from(message: L2ToL1Message) -> Self {
        starknet_api::transaction::MessageToL1 {
            from_address: message.from_address,
            to_address: message.to_address,
            payload: message.payload,
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
    #[serde(rename(deserialize = "INVOKE_FUNCTION", serialize = "INVOKE_FUNCTION"))]
    #[default]
    InvokeFunction,
    #[serde(rename(deserialize = "L1_HANDLER", serialize = "L1_HANDLER"))]
    L1Handler,
}
