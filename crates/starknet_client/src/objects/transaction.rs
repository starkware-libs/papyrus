use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_api::core::{
    ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, Nonce,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{
    Calldata, ContractAddressSalt, DeclareTransactionOutput, DeployAccountTransactionOutput,
    DeployTransactionOutput, EthAddress, Event, Fee, InvokeTransactionOutput,
    L1HandlerTransactionOutput, L1ToL2Payload, L2ToL1Payload, MessageToL1, TransactionHash,
    TransactionOffsetInBlock, TransactionOutput, TransactionSignature, TransactionVersion,
};

use crate::ClientError;

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

// TODO(dan): consider extracting common fields out (version, hash, type).
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
// Note: When deserializing an untagged enum, no variant can be a prefix of variants to follow.
pub enum Transaction {
    Declare(IntermediateDeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Deploy(DeployTransaction),
    Invoke(IntermediateInvokeTransaction),
    L1Handler(L1HandlerTransaction),
}

impl TryFrom<Transaction> for starknet_api::transaction::Transaction {
    type Error = ClientError;
    fn try_from(tx: Transaction) -> Result<Self, ClientError> {
        match tx {
            Transaction::Declare(declare_tx) => {
                Ok(starknet_api::transaction::Transaction::Declare(declare_tx.try_into()?))
            }
            Transaction::Deploy(deploy_tx) => {
                Ok(starknet_api::transaction::Transaction::Deploy(deploy_tx.into()))
            }
            Transaction::DeployAccount(deploy_acc_tx) => {
                Ok(starknet_api::transaction::Transaction::DeployAccount(deploy_acc_tx.into()))
            }
            Transaction::Invoke(invoke_tx) => {
                Ok(starknet_api::transaction::Transaction::Invoke(invoke_tx.try_into()?))
            }
            Transaction::L1Handler(l1_handler_tx) => {
                Ok(starknet_api::transaction::Transaction::L1Handler(l1_handler_tx.into()))
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

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct IntermediateDeclareTransaction {
    pub class_hash: ClassHash,
    pub compiled_class_hash: Option<CompiledClassHash>,
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
    pub signature: TransactionSignature,
    pub r#type: TransactionType,
}

impl TryFrom<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransaction {
    type Error = ClientError;

    fn try_from(declare_tx: IntermediateDeclareTransaction) -> Result<Self, ClientError> {
        match declare_tx.version {
            v if v == tx_v0() => Ok(Self::V0(declare_tx.into())),
            v if v == tx_v1() => Ok(Self::V1(declare_tx.into())),
            v if v == tx_v2() => Ok(Self::V2(declare_tx.try_into()?)),
            _ => Err(ClientError::BadTransaction {
                tx_hash: declare_tx.transaction_hash,
                msg: format!("Declare version {:?} is not supported.", declare_tx.version),
            }),
        }
    }
}

impl From<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransactionV0V1 {
    fn from(declare_tx: IntermediateDeclareTransaction) -> Self {
        Self {
            transaction_hash: declare_tx.transaction_hash,
            max_fee: declare_tx.max_fee,
            signature: declare_tx.signature,
            nonce: declare_tx.nonce,
            class_hash: declare_tx.class_hash,
            sender_address: declare_tx.sender_address,
        }
    }
}

impl TryFrom<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransactionV2 {
    type Error = ClientError;

    fn try_from(declare_tx: IntermediateDeclareTransaction) -> Result<Self, ClientError> {
        Ok(Self {
            transaction_hash: declare_tx.transaction_hash,
            max_fee: declare_tx.max_fee,
            signature: declare_tx.signature,
            nonce: declare_tx.nonce,
            class_hash: declare_tx.class_hash,
            compiled_class_hash: declare_tx.compiled_class_hash.ok_or(
                ClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V2 must contain compiled_class_hash field.".to_string(),
                },
            )?,
            sender_address: declare_tx.sender_address,
        })
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
pub struct IntermediateInvokeTransaction {
    pub calldata: Calldata,
    // In early versions of starknet, the `sender_address` field was originally named
    // `contract_address`.
    #[serde(alias = "contract_address")]
    pub sender_address: ContractAddress,
    pub entry_point_selector: Option<EntryPointSelector>,
    #[serde(default)]
    pub nonce: Option<Nonce>,
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    pub r#type: TransactionType,
}

impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransaction {
    type Error = ClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ClientError> {
        match invoke_tx.version {
            v if v == tx_v0() => Ok(Self::V0(invoke_tx.try_into()?)),
            v if v == tx_v1() => Ok(Self::V1(invoke_tx.try_into()?)),
            _ => Err(ClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: format!("Invoke version {:?} is not supported.", invoke_tx.version),
            }),
        }
    }
}

impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransactionV0 {
    type Error = ClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ClientError> {
        Ok(Self {
            transaction_hash: invoke_tx.transaction_hash,
            max_fee: invoke_tx.max_fee,
            signature: invoke_tx.signature,
            nonce: invoke_tx.nonce.unwrap_or_default(),
            sender_address: invoke_tx.sender_address,
            entry_point_selector: invoke_tx.entry_point_selector.ok_or(
                ClientError::BadTransaction {
                    tx_hash: invoke_tx.transaction_hash,
                    msg: "Invoke V0 must contain entry_point_selector field.".to_string(),
                },
            )?,
            calldata: invoke_tx.calldata,
        })
    }
}

impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransactionV1 {
    type Error = ClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ClientError> {
        // TODO(yair): Consider asserting that entry_point_selector is None.
        Ok(Self {
            transaction_hash: invoke_tx.transaction_hash,
            max_fee: invoke_tx.max_fee,
            signature: invoke_tx.signature,
            nonce: invoke_tx.nonce.ok_or(ClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V1 must contain nonce field.".to_string(),
            })?,
            sender_address: invoke_tx.sender_address,
            calldata: invoke_tx.calldata,
        })
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
            to_address: message.to_address,
            payload: message.payload,
            from_address: message.from_address,
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
