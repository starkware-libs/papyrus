use std::collections::HashMap;

use papyrus_storage::compression_utils::{CompressionError, GzEncoded};
use papyrus_storage::{StorageSerde, ThinStateDiff};
use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockHash, EntryPoint, EntryPointType, EventAbiEntry, FunctionAbiEntry, FunctionAbiEntryType,
    GlobalRoot, StructAbiEntry,
};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ContractClassAbiEntry {
    /// An event abi entry.
    Event(EventAbiEntry),
    /// A function abi entry.
    Function(FunctionAbiEntry),
    /// A struct abi entry.
    Struct(StructAbiEntry),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum ContractClassAbiEntryType {
    #[serde(rename(deserialize = "constructor", serialize = "constructor"))]
    Constructor,
    #[serde(rename(deserialize = "event", serialize = "event"))]
    Event,
    #[serde(rename(deserialize = "function", serialize = "function"))]
    Function,
    #[serde(rename(deserialize = "l1_handler", serialize = "l1_handler"))]
    L1Handler,
    #[serde(rename(deserialize = "struct", serialize = "struct"))]
    Struct,
}
impl Default for ContractClassAbiEntryType {
    fn default() -> Self {
        ContractClassAbiEntryType::Function
    }
}

impl From<FunctionAbiEntryType> for ContractClassAbiEntryType {
    fn from(t: FunctionAbiEntryType) -> Self {
        match t {
            FunctionAbiEntryType::Constructor => ContractClassAbiEntryType::Constructor,
            FunctionAbiEntryType::L1Handler => ContractClassAbiEntryType::L1Handler,
            FunctionAbiEntryType::Regular => ContractClassAbiEntryType::Function,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClassAbiEntryWithType {
    pub r#type: ContractClassAbiEntryType,
    #[serde(flatten)]
    pub entry: ContractClassAbiEntry,
}

impl From<starknet_api::ContractClassAbiEntry> for ContractClassAbiEntryWithType {
    fn from(entry: starknet_api::ContractClassAbiEntry) -> Self {
        match entry {
            starknet_api::ContractClassAbiEntry::Event(entry) => Self {
                r#type: ContractClassAbiEntryType::Event,
                entry: ContractClassAbiEntry::Event(entry),
            },
            starknet_api::ContractClassAbiEntry::Function(entry) => Self {
                r#type: entry.r#type.clone().into(),
                entry: ContractClassAbiEntry::Function(entry.entry),
            },
            starknet_api::ContractClassAbiEntry::Struct(entry) => Self {
                r#type: ContractClassAbiEntryType::Struct,
                entry: ContractClassAbiEntry::Struct(entry),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: Option<Vec<ContractClassAbiEntryWithType>>,
    /// A base64 encoding of the gzip-compressed JSON representation of program.
    pub program: String,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl TryFrom<starknet_api::ContractClass> for ContractClass {
    type Error = CompressionError;
    fn try_from(class: starknet_api::ContractClass) -> Result<Self, Self::Error> {
        // TODO(anatg): Deal with serde_json error.
        let mut program_value = serde_json::to_value(&class.program).unwrap();
        // Remove the 'attributes' key if it is null.
        if class.program.attributes == serde_json::value::Value::Null {
            program_value.as_object_mut().unwrap().remove("attributes");
        }
        // Remove the 'compiler_version' key if it is null.
        if class.program.compiler_version == serde_json::value::Value::Null {
            program_value.as_object_mut().unwrap().remove("compiler_version");
        }

        Ok(Self {
            abi: class.abi.map(|entries| entries.into_iter().map(|entry| entry.into()).collect()),
            program: base64::encode(GzEncoded::encode(Program(program_value))?),
            entry_points_by_type: class.entry_points_by_type,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: ThinStateDiff,
}

// The StorageSerde implementation for serde_json::Value writes the length (in bytes)
// of the value. Here we serialize the whole program as one value so no need to write
// its length.
pub struct Program(serde_json::Value);
impl StorageSerde for Program {
    /// Serializes the entire program as one json value.
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        // TODO(anatg): Deal with serde_json error.
        serde_json::to_writer(res, &self.0).unwrap();
        Ok(())
    }

    /// Deserializes the entire program as one json value.
    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let value = serde_json::from_reader(bytes).ok()?;
        Some(Program(value))
    }
}
