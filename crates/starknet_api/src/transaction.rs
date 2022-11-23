use serde::{Deserialize, Serialize};
use web3::types::H160;

use crate::block::{BlockHash, BlockNumber};
use crate::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use crate::hash::{StarkFelt, StarkHash};
use crate::serde_utils::PrefixedBytesAsHex;

/// A transaction.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transaction {
    /// A declare transaction.
    Declare(DeclareTransaction),
    /// A deploy transaction.
    Deploy(DeployTransaction),
    /// A deploy account transaction.
    DeployAccount(DeployAccountTransaction),
    /// An invoke transaction.
    Invoke(InvokeTransaction),
    /// An L1 handler transaction.
    L1Handler(L1HandlerTransaction),
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
}

/// A transaction output.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum TransactionOutput {
    /// A declare transaction output.
    Declare(DeclareTransactionOutput),
    /// A deploy transaction output.
    Deploy(DeployTransactionOutput),
    /// A deploy account transaction output.
    DeployAccount(DeployAccountTransactionOutput),
    /// An invoke transaction output.
    Invoke(InvokeTransactionOutput),
    /// An L1 handler transaction output.
    L1Handler(L1HandlerTransactionOutput),
}

impl TransactionOutput {
    pub fn actual_fee(&self) -> Fee {
        match self {
            TransactionOutput::Declare(output) => output.actual_fee,
            TransactionOutput::Deploy(output) => output.actual_fee,
            TransactionOutput::DeployAccount(output) => output.actual_fee,
            TransactionOutput::Invoke(output) => output.actual_fee,
            TransactionOutput::L1Handler(output) => output.actual_fee,
        }
    }

    pub fn events(&self) -> &[Event] {
        match self {
            TransactionOutput::Declare(output) => &output.events,
            TransactionOutput::Deploy(output) => &output.events,
            TransactionOutput::DeployAccount(output) => &output.events,
            TransactionOutput::Invoke(output) => &output.events,
            TransactionOutput::L1Handler(output) => &output.events,
        }
    }
}

/// A declare transaction.
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

/// A deploy account transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployAccountTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: CallData,
}

/// A deploy transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransaction {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    pub class_hash: ClassHash,
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: CallData,
}

/// An invoke transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransaction {
    pub transaction_hash: TransactionHash,
    pub max_fee: Fee,
    pub version: TransactionVersion,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    // An invoke transaction without an entry point selector invokes the 'execute' function.
    pub entry_point_selector: Option<EntryPointSelector>,
    pub calldata: CallData,
}

/// An L1 handler transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1HandlerTransaction {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: CallData,
}

/// A declare transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
}

/// A deploy-account transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployAccountTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
}

/// A deploy transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
}

/// An invoke transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
}

/// An L1 handler transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1HandlerTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
}

/// A transaction receipt.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

/// A fee.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct Fee(pub u128);

impl From<PrefixedBytesAsHex<16_usize>> for Fee {
    fn from(val: PrefixedBytesAsHex<16_usize>) -> Self {
        Self(u128::from_be_bytes(val.0))
    }
}

impl From<Fee> for PrefixedBytesAsHex<16_usize> {
    fn from(fee: Fee) -> Self {
        Self(fee.0.to_be_bytes())
    }
}

/// The hash of a [Transaction](`crate::transaction::Transaction`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionHash(pub StarkHash);

/// A contract address salt.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);

/// A transaction signature.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<StarkFelt>);

/// A transaction version.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionVersion(pub StarkFelt);

/// The calldata of a transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CallData(pub Vec<StarkFelt>);

/// An L1 to L2 message.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL2 {
    pub from_address: EthAddress,
    pub payload: L1ToL2Payload,
}

/// An L2 to L1 message.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

/// An Ethereum address.
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

/// An event.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Event {
    pub from_address: ContractAddress,
    #[serde(flatten)]
    pub content: EventContent,
}

/// An event content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventContent {
    pub keys: Vec<EventKey>,
    pub data: EventData,
}

/// An event key.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventKey(pub StarkFelt);

/// An event data.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<StarkFelt>);

/// The index of a transaction in [BlockBody](`crate::block::BlockBody`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionOffsetInBlock(pub usize);

/// The index of an event in [TransactionOutput](`crate::transaction::TransactionOutput`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EventIndexInTransactionOutput(pub usize);
