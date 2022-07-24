use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::{
    CallData, ClassHash, ContractAddress, ContractAddressSalt, EntryPointSelector, EthAddress,
    Event, Fee, L1ToL2Payload, L2ToL1Payload, Nonce, StarkHash, TransactionHash,
    TransactionSignature, TransactionVersion,
};

// TODO(dan): consider extracting common fields out (version, hash, type).
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum Transaction {
    Declare(DeclareTransaction),
    Deploy(DeployTransaction),
    Invoke(InvokeTransaction),
}

impl From<Transaction> for starknet_api::Transaction {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Declare(declare_tx) => {
                starknet_api::Transaction::Declare(declare_tx.into())
            }
            Transaction::Deploy(deploy_tx) => starknet_api::Transaction::Deploy(deploy_tx.into()),
            Transaction::Invoke(invoke_tx) => starknet_api::Transaction::Invoke(invoke_tx.into()),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
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

impl From<DeclareTransaction> for starknet_api::DeclareTransaction {
    fn from(declare_tx: DeclareTransaction) -> Self {
        starknet_api::DeclareTransaction {
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

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct DeployTransaction {
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: CallData,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

impl From<DeployTransaction> for starknet_api::DeployTransaction {
    fn from(deploy_tx: DeployTransaction) -> Self {
        starknet_api::DeployTransaction {
            transaction_hash: deploy_tx.transaction_hash,
            version: deploy_tx.version,
            contract_address: deploy_tx.contract_address,
            constructor_calldata: deploy_tx.constructor_calldata,
            class_hash: deploy_tx.class_hash,
            contract_address_salt: deploy_tx.contract_address_salt,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct InvokeTransaction {
    pub calldata: CallData,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub entry_point_type: EntryPointType,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

impl From<InvokeTransaction> for starknet_api::InvokeTransaction {
    fn from(invoke_tx: InvokeTransaction) -> Self {
        starknet_api::InvokeTransaction {
            transaction_hash: invoke_tx.transaction_hash,
            max_fee: invoke_tx.max_fee,
            version: invoke_tx.version,
            signature: invoke_tx.signature,
            // TODO(anatg): Get the real nonce when the sequencer returns one.
            nonce: Nonce::default(),
            contract_address: invoke_tx.contract_address,
            entry_point_selector: invoke_tx.entry_point_selector,
            call_data: invoke_tx.calldata,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct TransactionReceipt {
    pub transaction_index: TransactionIndexInBlock,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub l1_to_l2_consumed_message: L1ToL2Message,
    pub l2_to_l1_messages: Vec<L2ToL1Message>,
    pub events: Vec<Event>,
    pub execution_resources: ExecutionResources,
    pub actual_fee: Fee,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct ExecutionResources {
    pub n_steps: u64,
    pub builtin_instance_counter: BuiltinInstanceCounter,
    pub n_memory_holes: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
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

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct EmptyBuiltinInstanceCounter {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum EntryPointType {
    #[serde(rename(deserialize = "EXTERNAL", serialize = "EXTERNAL"))]
    External,
    #[serde(rename(deserialize = "L1_HANDLER", serialize = "L1_HANDLER"))]
    L1Handler,
    #[serde(rename(deserialize = "CONSTRUCTOR", serialize = "CONSTRUCTOR"))]
    Constructor,
}
impl Default for EntryPointType {
    fn default() -> Self {
        EntryPointType::External
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Nonce(pub StarkHash);

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionIndexInBlock(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "INVOKE_FUNCTION", serialize = "INVOKE_FUNCTION"))]
    InvokeFunction,
}
impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::InvokeFunction
    }
}
