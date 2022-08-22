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
pub struct TransactionOffsetInBlock(pub u64);

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
    from_address: ContractAddress,
    keys: Vec<EventKey>,
    data: EventData,
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
    selector: EntryPointSelector,
    offset: EntryPointOffset,
}

impl EntryPoint {
    pub fn new(selector: EntryPointSelector, offset: EntryPointOffset) -> Self {
        Self { selector, offset }
    }
}

/// A program corresponding to a contract class in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Program {
    #[serde(default)]
    attributes: serde_json::Value,
    builtins: serde_json::Value,
    data: serde_json::Value,
    debug_info: serde_json::Value,
    hints: serde_json::Value,
    identifiers: serde_json::Value,
    main_scope: serde_json::Value,
    prime: serde_json::Value,
    reference_manager: serde_json::Value,
}

impl Program {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        attributes: serde_json::Value,
        builtins: serde_json::Value,
        data: serde_json::Value,
        debug_info: serde_json::Value,
        hints: serde_json::Value,
        identifiers: serde_json::Value,
        main_scope: serde_json::Value,
        prime: serde_json::Value,
        reference_manager: serde_json::Value,
    ) -> Self {
        Self {
            attributes,
            builtins,
            data,
            debug_info,
            hints,
            identifiers,
            main_scope,
            prime,
            reference_manager,
        }
    }
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
    abi: serde_json::Value,
    program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
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

    pub fn new(
        abi: serde_json::Value,
        program: Program,
        entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
    ) -> Self {
        Self { abi, program, entry_points_by_type }
    }

    pub fn set_abi(&mut self, abi: serde_json::Value) {
        self.abi = abi;
    }
}

/// A declare transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransaction {
    transaction_hash: TransactionHash,
    max_fee: Fee,
    version: TransactionVersion,
    signature: TransactionSignature,
    nonce: Nonce,
    class_hash: ClassHash,
    sender_address: ContractAddress,
}

impl DeclareTransaction {
    pub fn new(
        transaction_hash: TransactionHash,
        max_fee: Fee,
        version: TransactionVersion,
        signature: TransactionSignature,
        nonce: Nonce,
        class_hash: ClassHash,
        sender_address: ContractAddress,
    ) -> Self {
        Self { transaction_hash, max_fee, version, signature, nonce, class_hash, sender_address }
    }
}

/// An invoke transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransaction {
    transaction_hash: TransactionHash,
    max_fee: Fee,
    version: TransactionVersion,
    signature: TransactionSignature,
    nonce: Nonce,
    contract_address: ContractAddress,
    entry_point_selector: EntryPointSelector,
    call_data: CallData,
}

impl InvokeTransaction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transaction_hash: TransactionHash,
        max_fee: Fee,
        version: TransactionVersion,
        signature: TransactionSignature,
        nonce: Nonce,
        contract_address: ContractAddress,
        entry_point_selector: EntryPointSelector,
        call_data: CallData,
    ) -> Self {
        Self {
            transaction_hash,
            max_fee,
            version,
            signature,
            nonce,
            contract_address,
            entry_point_selector,
            call_data,
        }
    }
}

/// A contract address salt in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);

/// A deploy transaction in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransaction {
    transaction_hash: TransactionHash,
    version: TransactionVersion,
    class_hash: ClassHash,
    contract_address: ContractAddress,
    contract_address_salt: ContractAddressSalt,
    constructor_calldata: CallData,
}

impl DeployTransaction {
    pub fn new(
        transaction_hash: TransactionHash,
        version: TransactionVersion,
        class_hash: ClassHash,
        contract_address: ContractAddress,
        contract_address_salt: ContractAddressSalt,
        constructor_calldata: CallData,
    ) -> Self {
        Self {
            transaction_hash,
            version,
            class_hash,
            contract_address,
            contract_address_salt,
            constructor_calldata,
        }
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

/// An L1 to L2 message in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL2 {
    from_address: EthAddress,
    payload: L1ToL2Payload,
}

/// An L2 to L1 message in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL1 {
    to_address: EthAddress,
    payload: L2ToL1Payload,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StatusData(pub Vec<StarkFelt>);

/// A transaction receipt in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionReceipt {
    transaction_hash: TransactionHash,
    block_hash: BlockHash,
    block_number: BlockNumber,
    output: TransactionOutput,
}

impl TransactionReceipt {
    pub fn new(
        transaction_hash: TransactionHash,
        block_hash: BlockHash,
        block_number: BlockNumber,
        output: TransactionOutput,
    ) -> Self {
        Self { transaction_hash, block_hash, block_number, output }
    }
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
