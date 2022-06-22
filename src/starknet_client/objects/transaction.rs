use serde::{Deserialize, Serialize};

use crate::starknet::{
    CallData, ClassHash, ContractAddress, EntryPointSelector, EthAddress, Event, Fee,
    L1ToL2Payload, L2ToL1Payload, Nonce, StarkHash, TransactionHash,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Transaction {
    Declare(DeclareTransaction),
    Deploy(DeployTransaction),
    Invoke(InvokeTransaction),
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct DeclareTransaction {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
    pub signature: TransactionSignature,
    pub r#type: TransactionType,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct DeployTransaction {
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: CallData,
    pub transaction_hash: TransactionHash,
    pub r#type: TransactionType,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct InvokeTransaction {
    pub calldata: CallData,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub entry_point_type: EntryPointType,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub transaction_hash: TransactionHash,
    pub r#type: TransactionType,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TransactionReceipt {
    pub transaction_index: TransactionIndexInBlock,
    pub transaction_hash: TransactionHash,
    pub l1_to_l2_consumed_message: Option<L1ToL2Message>,
    pub l2_to_l1_messages: Vec<L2ToL1Message>,
    pub events: Vec<Event>,
    // TODO(dan): define corresponding struct and handle properly.
    pub execution_resources: serde_json::Value,
    pub actual_fee: Fee,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);

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

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionIndexInBlock(pub u32);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<StarkHash>);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(
        deserialize = "INITIALIZE_BLOCK_INFO",
        serialize = "INITIALIZE_BLOCK_INFO"
    ))]
    InitializeBlockInfo,
    #[serde(rename(deserialize = "INVOKE_FUNCTION", serialize = "INVOKE_FUNCTION"))]
    InvokeFunction,
}
impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::InvokeFunction
    }
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionVersion(pub StarkHash);
