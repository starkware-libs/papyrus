#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::collections::HashMap;
use std::fmt::Debug;
use std::mem;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::block::{BlockHash, BlockNumber};
use crate::core::{ClassHash, ContractAddress, EntryPointSelector, GlobalRoot, Nonce, PatriciaKey};
use crate::hash::{StarkFelt, StarkHash};
use crate::serde_utils::bytes_from_hex_str;
use crate::StarknetApiError;

/// The differences between two states before and after a block with hash block_hash
/// and their respective roots.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}

/// The differences between two states.
// Invariant: Addresses are strictly increasing.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateDiff {
    pub deployed_contracts: IndexMap<ContractAddress, ClassHash>,
    pub storage_diffs: IndexMap<ContractAddress, IndexMap<StorageKey, StarkFelt>>,
    pub declared_classes: IndexMap<ClassHash, ContractClass>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
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
    // TODO: Consider changing to IndexMap, since this is used for computing the
    // class hash.
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
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(try_from = "String", into = "String")]
pub struct EntryPointOffset(pub usize);

impl TryFrom<String> for EntryPointOffset {
    type Error = StarknetApiError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const SIZE_OF_USIZE: usize = mem::size_of::<usize>();
        let bytes = bytes_from_hex_str::<SIZE_OF_USIZE, true>(value.as_str())?;
        Ok(Self(usize::from_be_bytes(bytes)))
    }
}

impl From<EntryPointOffset> for String {
    fn from(value: EntryPointOffset) -> Self {
        format!("0x{:x}", value.0)
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
