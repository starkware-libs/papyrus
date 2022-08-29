use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use web3::types::H160;

use super::serde_utils::PrefixedHexAsBytes;
use super::{BlockHash, BlockNumber, ClassHash, ContractAddress, Nonce, StarkFelt, StarkHash};

/// The hash of a transaction in a StarkNet.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionHash(pub StarkHash);

/// The index of a transaction in a StarkNet [`BlockBody`](super::BlockBody).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionOffsetInBlock(pub usize);

/// A fee in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
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

/// An event data in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<StarkFelt>);

/// An event key in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventKey(pub StarkFelt);

/// An event in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Event {
    pub from_address: ContractAddress,
    pub keys: Vec<EventKey>,
    pub data: EventData,
}

/// The selector of an entry point in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointSelector(pub StarkHash);

/// The offset of an entry point in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointOffset(pub StarkFelt);

/// An entry point of a contract in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
}

/// A program corresponding to a contract class in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
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

/// The calldata of a transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CallData(pub Vec<StarkFelt>);

/// An Ethereum address in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EthAddress(pub H160);

/// The payload of [`MessageToL2`].
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Payload(pub Vec<StarkFelt>);

/// The payload of [`MessageToL1`].
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L2ToL1Payload(pub Vec<StarkFelt>);

/// A transaction version in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionVersion(pub StarkFelt);

/// A transaction signature in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<StarkFelt>);

/// An entry point type of a contract in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    /// A constructor entry point.
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    /// An external4 entry point.
    #[serde(rename = "EXTERNAL")]
    External,
    /// An L1 handler entry point.
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

impl Default for EntryPointType {
    fn default() -> Self {
        EntryPointType::L1Handler
    }
}

/// A contract class in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: serde_json::Value,
    pub program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl ContractClass {
    /// Returns a byte vector representation of a contract class.
    pub fn to_byte_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Bytes from contract class")
    }

    /// Returns a contract class corresponding to the given byte vector.
    pub fn from_byte_vec(byte_vec: &[u8]) -> ContractClass {
        serde_json::from_slice::<ContractClass>(byte_vec).expect("Contract class from bytes")
    }
}

/// A declare transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
}

/// An invoke transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub call_data: CallData,
}

/// A contract address salt in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);

/// A deploy transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransaction {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    pub class_hash: ClassHash,
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: CallData,
}

/// An L1 to L2 message in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL2 {
    pub from_address: EthAddress,
    pub payload: L1ToL2Payload,
}

/// An L2 to L1 message in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

/// A transaction receipt in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

/// A transaction output in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionOutput {
    /// A declare transaction output.
    Declare(DeclareTransactionOutput),
    /// A deploy transaction output.
    Deploy(DeployTransactionOutput),
    /// An invoke transaction output.
    Invoke(InvokeTransactionOutput),
}

/// An invoke transaction output in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub l1_origin_message: Option<MessageToL2>,
    pub events: Vec<Event>,
}

/// A declare transaction output in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionOutput {
    pub actual_fee: Fee,
}

/// A deploy transaction output in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransactionOutput {
    pub actual_fee: Fee,
}

impl TransactionOutput {
    pub fn actual_fee(&self) -> Fee {
        match self {
            TransactionOutput::Declare(output) => output.actual_fee,
            TransactionOutput::Deploy(output) => output.actual_fee,
            TransactionOutput::Invoke(output) => output.actual_fee,
        }
    }
}

/// A transaction in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transaction {
    /// A declare transaction.
    Declare(DeclareTransaction),
    /// A deploy transaction.
    Deploy(DeployTransaction),
    /// An invoke transaction.
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
