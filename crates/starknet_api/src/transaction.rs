use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use web3::types::H160;

use super::serde_utils::PrefixedHexAsBytes;
use super::{BlockHash, BlockNumber, ClassHash, ContractAddress, StarkFelt, StarkHash};

#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionHash(pub StarkHash);

// Index of a transaction inside a block.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionOffsetInBlock(pub u64);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedHexAsBytes<16_usize>", into = "PrefixedHexAsBytes<16_usize>")]
pub struct Fee(pub u128);
impl From<PrefixedHexAsBytes<16_usize>> for Fee {
    fn from(val: PrefixedHexAsBytes<16_usize>) -> Self {
        Self(u128::from_be_bytes(val.0))
    }
}
impl From<Fee> for PrefixedHexAsBytes<16_usize> {
    fn from(fee: Fee) -> Self {
        Self(fee.0.to_be_bytes())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<StarkFelt>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventKey(pub StarkFelt);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Event {
    pub from_address: ContractAddress,
    pub keys: Vec<EventKey>,
    pub data: EventData,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointSelector(pub StarkHash);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointOffset(pub StarkFelt);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Program {
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub builtins: serde_json::Value,
    pub data: serde_json::Value,
    pub debug_info: serde_json::Value,
    pub hints: serde_json::Value,
    pub identifiers: serde_json::Value,
    pub main_scope: serde_json::Value,
    pub prime: serde_json::Value,
    pub reference_manager: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CallData(pub Vec<StarkFelt>);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EthAddress(pub H160);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Payload(pub Vec<StarkFelt>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L2ToL1Payload(pub Vec<StarkFelt>);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionVersion(pub StarkFelt);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<StarkFelt>);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    #[serde(rename = "EXTERNAL")]
    External,
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

impl Default for EntryPointType {
    fn default() -> Self {
        EntryPointType::L1Handler
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: serde_json::Value,
    pub program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl From<Vec<u8>> for ContractClass {
    fn from(val: Vec<u8>) -> Self {
        serde_json::from_slice::<ContractClass>(&val).expect("Contract class from bytes")
    }
}

impl From<ContractClass> for Vec<u8> {
    fn from(contract_class: ContractClass) -> Self {
        serde_json::to_vec(&contract_class).expect("Bytes from contract class")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub call_data: CallData,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub contract_address: ContractAddress,
    pub constructor_calldata: CallData,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::AcceptedOnL2
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL2 {
    pub from_address: EthAddress,
    pub payload: L1ToL2Payload,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StatusData(pub Vec<StarkFelt>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub actual_fee: Fee,
    pub status: TransactionStatus,
    pub status_data: StatusData,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub messages_sent: Vec<MessageToL1>,
    pub l1_origin_message: Option<MessageToL2>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub actual_fee: Fee,
    pub status: TransactionStatus,
    pub status_data: StatusData,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub actual_fee: Fee,
    pub status: TransactionStatus,
    pub status_data: StatusData,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
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
            Transaction::Invoke(tx) => tx.transaction_hash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionReceipt {
    Declare(DeclareTransactionReceipt),
    Deploy(DeployTransactionReceipt),
    Invoke(InvokeTransactionReceipt),
}
