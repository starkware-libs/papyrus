use serde::{Deserialize, Serialize};
use web3::types::H160;

use crate::starknet;

use super::serde_utils::{HexAsBytes, NonPrefixedHexAsBytes, PrefixedHexAsBytes};

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(
    from = "PrefixedHexAsBytes<32_usize>",
    into = "PrefixedHexAsBytes<32_usize>"
)]
pub struct StarkHash(pub [u8; 32]);
impl From<PrefixedHexAsBytes<32_usize>> for StarkHash {
    fn from(val: PrefixedHexAsBytes<32_usize>) -> Self {
        StarkHash(val.0)
    }
}
impl From<StarkHash> for PrefixedHexAsBytes<32_usize> {
    fn from(val: StarkHash) -> Self {
        HexAsBytes(val.0)
    }
}
impl From<StarkHash> for starknet::StarkHash {
    fn from(val: StarkHash) -> Self {
        starknet::StarkHash(val.0)
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHash(pub StarkHash);
impl From<BlockHash> for starknet::BlockHash {
    fn from(val: BlockHash) -> Self {
        starknet::BlockHash(val.0.into())
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);
impl From<ContractAddress> for starknet::ContractAddress {
    fn from(val: ContractAddress) -> Self {
        starknet::ContractAddress(val.0.into())
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(
    from = "NonPrefixedHexAsBytes<32_usize>",
    into = "NonPrefixedHexAsBytes<32_usize>"
)]
pub struct GlobalRoot(pub StarkHash);
// We don't use the regular StarkHash deserialization since the Starknet sequencer returns the
// global root hash as a hex string without a "0x" prefix.
impl From<NonPrefixedHexAsBytes<32_usize>> for GlobalRoot {
    fn from(val: NonPrefixedHexAsBytes<32_usize>) -> Self {
        GlobalRoot(StarkHash(val.0))
    }
}
impl From<GlobalRoot> for NonPrefixedHexAsBytes<32_usize> {
    fn from(val: GlobalRoot) -> Self {
        HexAsBytes(val.0 .0)
    }
}
impl From<GlobalRoot> for starknet::GlobalRoot {
    fn from(val: GlobalRoot) -> Self {
        starknet::GlobalRoot(val.0.into())
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockNumber(pub u64);
impl From<BlockNumber> for starknet::BlockNumber {
    fn from(val: BlockNumber) -> Self {
        starknet::BlockNumber(val.0)
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(
    from = "PrefixedHexAsBytes<16_usize>",
    into = "PrefixedHexAsBytes<16_usize>"
)]
pub struct GasPrice(pub u128);
impl From<PrefixedHexAsBytes<16_usize>> for GasPrice {
    fn from(val: PrefixedHexAsBytes<16_usize>) -> Self {
        GasPrice(u128::from_be_bytes(val.0))
    }
}
impl From<GasPrice> for PrefixedHexAsBytes<16_usize> {
    fn from(val: GasPrice) -> Self {
        HexAsBytes(val.0.to_be_bytes())
    }
}
impl From<GasPrice> for starknet::GasPrice {
    fn from(val: GasPrice) -> Self {
        starknet::GasPrice(val.0)
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub u64);
impl From<BlockTimestamp> for starknet::BlockTimestamp {
    fn from(val: BlockTimestamp) -> Self {
        starknet::BlockTimestamp(val.0)
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct CallData(pub Vec<StarkHash>);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointSelector(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ClassHash(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedHexAsBytes<16_usize>")]
pub struct MaxFee(pub u128);
impl From<PrefixedHexAsBytes<16_usize>> for MaxFee {
    fn from(val: PrefixedHexAsBytes<16_usize>) -> Self {
        MaxFee(u128::from_be_bytes(val.0))
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<StarkHash>);
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Payload(pub Vec<StarkHash>);
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<StarkHash>);
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventKey(pub Vec<StarkHash>);
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Nonce(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionHash(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionNonce(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionVersion(pub StarkHash);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionIndexInBlock(pub u32);
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EthAddress(pub H160);
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Transaction {
    Invoke(InvokeTransaction),
    Deploy(DeployTransaction),
    Declare(DeclareTransaction),
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
    pub max_fee: MaxFee,
    pub signature: TransactionSignature,
    pub transaction_hash: TransactionHash,
    pub r#type: TransactionType,
}
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct DeclareTransaction {
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
    pub nonce: TransactionNonce,
    pub max_fee: MaxFee,
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
    pub signature: TransactionSignature,
    pub r#type: TransactionType,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    pub nonce: L1ToL2Nonce,
}
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L1ToL2Payload,
}
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Event {
    pub from_address: ContractAddress,
    pub keys: EventKey,
    pub data: EventData,
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
    pub actual_fee: MaxFee,
}
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Block {
    // TODO(dan): Currently should be Option<BlockHash> (due to pending blocks).
    // Figure out if we want this in the internal representation as well.
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub parent_block_hash: BlockHash,
    #[serde(default)]
    pub sequencer_address: ContractAddress,
    pub state_root: GlobalRoot,
    pub status: BlockStatus,
    #[serde(default)]
    pub timestamp: BlockTimestamp,
    pub transactions: Vec<Transaction>,
    pub transaction_receipts: Vec<TransactionReceipt>,
}

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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename(deserialize = "ABORTED", serialize = "ABORTED"))]
    Aborted,
    #[serde(rename(deserialize = "ACCEPTED_ON_L1", serialize = "ACCEPTED_ON_L1"))]
    AcceptedOnL1,
    #[serde(rename(deserialize = "ACCEPTED_ON_L2", serialize = "ACCEPTED_ON_L2"))]
    AcceptedOnL2,
    #[serde(rename(deserialize = "PENDING", serialize = "PENDING"))]
    Pending,
    #[serde(rename(deserialize = "REVERTED", serialize = "REVERTED"))]
    Reverted,
}
impl Default for BlockStatus {
    fn default() -> Self {
        BlockStatus::AcceptedOnL2
    }
}
