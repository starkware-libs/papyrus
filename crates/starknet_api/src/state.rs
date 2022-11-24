#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::collections::HashMap;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::block::BlockNumber;
use crate::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce, PatriciaKey};
use crate::hash::{StarkFelt, StarkHash};
use crate::serde_utils::InnerDeserializationError;
use crate::StarknetApiError;

/// The differences between two states.
// Invariant: Addresses are strictly increasing.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateDiff {
    deployed_contracts: Vec<DeployedContract>,
    storage_diffs: Vec<StorageDiff>,
    declared_classes: Vec<DeclaredContract>,
    nonces: Vec<ContractNonce>,
}

impl StateDiff {
    /// Creates a new [StateDiff](`crate::state::StateDiff`).
    /// Sorts each vector by the addresses and verifies that there are no duplicate addresses.
    pub fn new(
        mut deployed_contracts: Vec<DeployedContract>,
        mut storage_diffs: Vec<StorageDiff>,
        mut declared_contracts: Vec<DeclaredContract>,
        mut nonces: Vec<ContractNonce>,
    ) -> Result<Self, StarknetApiError> {
        deployed_contracts.sort_unstable_by_key(|dc| dc.address);
        storage_diffs.sort_unstable_by_key(|sd| sd.address);
        declared_contracts.sort_unstable_by_key(|dc| dc.class_hash);
        nonces.sort_unstable_by_key(|n| n.contract_address);

        if !is_unique(&deployed_contracts, |dc| &dc.address) {
            return Err(StarknetApiError::DuplicateInStateDiff {
                object: "deployed_contracts".to_string(),
            });
        }

        if !is_unique(&storage_diffs, |sd| &sd.address) {
            return Err(StarknetApiError::DuplicateInStateDiff {
                object: "storage_diffs".to_string(),
            });
        }

        if !is_unique(&declared_contracts, |dc| &dc.class_hash) {
            return Err(StarknetApiError::DuplicateInStateDiff {
                object: "declared_contracts".to_string(),
            });
        }

        if !is_unique(&nonces, |contract_nonce| &contract_nonce.contract_address) {
            return Err(StarknetApiError::DuplicateInStateDiff { object: "nonces".to_string() });
        }

        Ok(Self { deployed_contracts, storage_diffs, declared_classes: declared_contracts, nonces })
    }

    pub fn deployed_contracts(&self) -> &[DeployedContract] {
        &self.deployed_contracts
    }

    pub fn storage_diffs(&self) -> &[StorageDiff] {
        &self.storage_diffs
    }

    pub fn declared_contracts(&self) -> &[DeclaredContract] {
        &self.declared_classes
    }

    pub fn nonces(&self) -> &[ContractNonce] {
        &self.nonces
    }
}

type StateDiffAsTuple =
    (Vec<DeployedContract>, Vec<StorageDiff>, Vec<DeclaredContract>, Vec<ContractNonce>);

impl From<StateDiff> for StateDiffAsTuple {
    fn from(diff: StateDiff) -> StateDiffAsTuple {
        (diff.deployed_contracts, diff.storage_diffs, diff.declared_classes, diff.nonces)
    }
}

/// The nonce of a [DeployedContract](`crate::state::DeployedContract`).
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractNonce {
    pub contract_address: ContractAddress,
    pub nonce: Nonce,
}

/// A deployed contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

/// A declared contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclaredContract {
    pub class_hash: ClassHash,
    pub contract_class: ContractClass,
}

/// Storage differences of a [DeployedContract](`crate::state::DeployedContract`).
// Invariant: Storage keys are strictly increasing. In particular, no key appears twice.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    storage_entries: Vec<StorageEntry>,
}

impl StorageDiff {
    /// Creates a new [StorageDiff](`crate::state::StorageDiff`).
    /// Sorts the storage entries by their key and verifies that there are no duplicate entries.
    pub fn new(
        address: ContractAddress,
        mut storage_entries: Vec<StorageEntry>,
    ) -> Result<Self, StarknetApiError> {
        storage_entries.sort_unstable_by_key(|se| se.key);
        if !is_unique(storage_entries.as_slice(), |se| &se.key) {
            return Err(StarknetApiError::DuplicateStorageEntry);
        }
        Ok(Self { address, storage_entries })
    }

    pub fn storage_entries(&self) -> &[StorageEntry] {
        &self.storage_entries
    }
}

/// The sequential numbering of the states between blocks.
// Example:
// States: S0       S1       S2
// Blocks      B0->     B1->
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StateNumber(pub BlockNumber);

impl StateNumber {
    /// The state at the beginning of the block.
    pub fn right_before_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number)
    }

    /// The state at the end of the block.
    pub fn right_after_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number.next())
    }

    pub fn is_before(&self, block_number: BlockNumber) -> bool {
        self.0 <= block_number
    }

    pub fn is_after(&self, block_number: BlockNumber) -> bool {
        !self.is_before(block_number)
    }

    pub fn block_after(&self) -> BlockNumber {
        self.0
    }
}

/// A contract class.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: Option<Vec<ContractClassAbiEntry>>,
    pub program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

/// An entry point type of a [ContractClass](`crate::state::ContractClass`).
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

/// An entry point of a [ContractClass](`crate::state::ContractClass`).
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
}

/// The offset of an [EntryPoint](`crate::state::EntryPoint`).
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct EntryPointOffset(pub usize);

impl<'de> Deserialize<'de> for EntryPointOffset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_str = String::deserialize(deserializer)?;
        let without_prefix = hex_str
            .strip_prefix("0x")
            .ok_or(InnerDeserializationError::MissingPrefix { hex_str: hex_str.clone() })
            .map_err(serde::de::Error::custom)?;
        usize::from_str_radix(without_prefix, 16).map_err(serde::de::Error::custom).map(Self)
    }
}

impl Serialize for EntryPointOffset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex_str = format!("0x{:x}", self.0);
        serializer.serialize_str(hex_str.as_str())
    }
}

/// A program corresponding to a [ContractClass](`crate::state::ContractClass`).
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Program {
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub builtins: serde_json::Value,
    #[serde(default)]
    pub compiler_version: serde_json::Value,
    pub data: serde_json::Value,
    pub debug_info: serde_json::Value,
    pub hints: serde_json::Value,
    pub identifiers: serde_json::Value,
    pub main_scope: serde_json::Value,
    pub prime: serde_json::Value,
    pub reference_manager: serde_json::Value,
}

/// A [ContractClass](`crate::state::ContractClass`) abi entry.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ContractClassAbiEntry {
    /// An event abi entry.
    Event(EventAbiEntry),
    /// A function abi entry.
    Function(FunctionAbiEntryWithType),
    /// A struct abi entry.
    Struct(StructAbiEntry),
}

/// An event abi entry.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EventAbiEntry {
    pub name: String,
    pub keys: Vec<TypedParameter>,
    pub data: Vec<TypedParameter>,
}

/// A function abi entry with type.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FunctionAbiEntryWithType {
    pub r#type: FunctionAbiEntryType,
    #[serde(flatten)]
    pub entry: FunctionAbiEntry,
}

/// A function abi entry type.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum FunctionAbiEntryType {
    #[serde(rename = "constructor")]
    Constructor,
    #[serde(rename = "l1_handler")]
    L1Handler,
    #[serde(rename = "regular")]
    Regular,
}

impl Default for FunctionAbiEntryType {
    fn default() -> Self {
        FunctionAbiEntryType::Regular
    }
}

/// A function abi entry.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FunctionAbiEntry {
    pub name: String,
    pub inputs: Vec<TypedParameter>,
    pub outputs: Vec<TypedParameter>,
}

/// A struct abi entry.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructAbiEntry {
    pub name: String,
    pub size: usize,
    pub members: Vec<StructMember>,
}

/// A struct member for [StructAbiEntry](`crate::state::StructAbiEntry`).
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructMember {
    #[serde(flatten)]
    pub param: TypedParameter,
    pub offset: usize,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct TypedParameter {
    pub name: String,
    pub r#type: String,
}

/// A storage key in a contract.
#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StorageKey(pub PatriciaKey);

impl TryFrom<StarkHash> for StorageKey {
    type Error = StarknetApiError;

    fn try_from(val: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::try_from(val)?))
    }
}

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}

fn is_unique<T, B, F>(sorted: &[T], f: F) -> bool
where
    F: Fn(&T) -> &B,
    B: PartialEq,
{
    sorted.windows(2).all(|w| f(&w[0]) != f(&w[1]))
}
