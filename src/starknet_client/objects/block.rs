use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::starknet;

use super::super::serde_utils::{HexAsBytes, NonPrefixedHexAsBytes, PrefixedHexAsBytes};
use super::transactions::{ClassHash, Transaction, TransactionReceipt};
use super::StarkHash;

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

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct BlockStateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
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
pub struct BlockNumber(pub u64);
impl From<BlockNumber> for starknet::BlockNumber {
    fn from(val: BlockNumber) -> Self {
        starknet::BlockNumber(val.0)
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

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub u64);
impl From<BlockTimestamp> for starknet::BlockTimestamp {
    fn from(val: BlockTimestamp) -> Self {
        starknet::BlockTimestamp(val.0)
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

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
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

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct StateDiff {
    pub storage_diffs: HashMap<ContractAddress, Vec<StorageEntry>>,
    pub deployed_contracts: Vec<DeployedContract>,
    // TODO(dan): define corresponding struct and handle properly.
    #[serde(default)]
    pub declared_contracts: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StorageValue,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StorageKey(pub StarkHash);

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StorageValue(pub StarkHash);
