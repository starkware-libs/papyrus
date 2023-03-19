use serde::{Deserialize, Serialize};
use starknet_api::state::{EventAbiEntry, StructAbiEntry};

/// A function abi entry with type.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct FunctionAbiEntryWithType {
    pub r#type: FunctionAbiEntryType,
    #[serde(flatten)]
    pub entry: starknet_api::state::FunctionAbiEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "stateMutability")]
    pub state_mutability: Option<String>,
}

/// A function abi entry type.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum FunctionAbiEntryType {
    #[serde(rename = "constructor")]
    Constructor,
    #[serde(rename = "l1_handler")]
    L1Handler,
    #[serde(rename = "function")]
    #[default]
    Function,
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