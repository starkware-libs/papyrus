use std::collections::HashMap;

use papyrus_storage::compression_utils::{CompressionError, GzEncoded};
use papyrus_storage::db::serialization::{StorageSerde, StorageSerdeError};
use papyrus_storage::state::data::ThinStateDiff as papyrus_storage_ThinStateDiff;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::core::{ClassHash, ContractAddress, GlobalRoot, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{
    EntryPoint, EntryPointType, EventAbiEntry, FunctionAbiEntry, FunctionAbiEntryType, StorageKey,
    StructAbiEntry,
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

#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum ContractClassAbiEntryType {
    #[serde(rename(deserialize = "constructor", serialize = "constructor"))]
    Constructor,
    #[serde(rename(deserialize = "event", serialize = "event"))]
    Event,
    #[serde(rename(deserialize = "function", serialize = "function"))]
    #[default]
    Function,
    #[serde(rename(deserialize = "l1_handler", serialize = "l1_handler"))]
    L1Handler,
    #[serde(rename(deserialize = "struct", serialize = "struct"))]
    Struct,
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

impl From<starknet_api::state::ContractClassAbiEntry> for ContractClassAbiEntryWithType {
    fn from(entry: starknet_api::state::ContractClassAbiEntry) -> Self {
        match entry {
            starknet_api::state::ContractClassAbiEntry::Event(entry) => Self {
                r#type: ContractClassAbiEntryType::Event,
                entry: ContractClassAbiEntry::Event(entry),
            },
            starknet_api::state::ContractClassAbiEntry::Function(entry) => Self {
                r#type: entry.r#type.clone().into(),
                entry: ContractClassAbiEntry::Function(entry.entry),
            },
            starknet_api::state::ContractClassAbiEntry::Struct(entry) => Self {
                r#type: ContractClassAbiEntryType::Struct,
                entry: ContractClassAbiEntry::Struct(entry),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: Vec<ContractClassAbiEntryWithType>,
    /// A base64 encoding of the gzip-compressed JSON representation of program.
    pub program: String,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl TryFrom<starknet_api::state::ContractClass> for ContractClass {
    type Error = CompressionError;
    fn try_from(class: starknet_api::state::ContractClass) -> Result<Self, Self::Error> {
        let mut program_value = serde_json::to_value(&class.program)
            .map_err(|err| CompressionError::StorageSerde(StorageSerdeError::Serde(err)))?;
        // Remove the 'attributes' key if it is null.
        if class.program.attributes == serde_json::value::Value::Null {
            program_value.as_object_mut().unwrap().remove("attributes");
        }
        // Remove the 'compiler_version' key if it is null.
        if class.program.compiler_version == serde_json::value::Value::Null {
            program_value.as_object_mut().unwrap().remove("compiler_version");
        }

        let abi = if class.abi.is_none() {
            vec![]
        } else {
            class.abi.unwrap().into_iter().map(|entry| entry.into()).collect()
        };

        Ok(Self {
            abi,
            program: base64::encode(GzEncoded::encode(Program(program_value))?),
            entry_points_by_type: class.entry_points_by_type,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: ThinStateDiff,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinStateDiff {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
    pub declared_contract_hashes: Vec<ClassHash>,
    pub nonces: Vec<ContractNonce>,
}

impl From<papyrus_storage_ThinStateDiff> for ThinStateDiff {
    fn from(diff: papyrus_storage_ThinStateDiff) -> Self {
        Self {
            deployed_contracts: Vec::from_iter(
                diff.deployed_contracts
                    .into_iter()
                    .map(|(address, class_hash)| DeployedContract { address, class_hash }),
            ),
            storage_diffs: Vec::from_iter(diff.storage_diffs.into_iter().map(
                |(address, entries)| {
                    let storage_entries = Vec::from_iter(
                        entries.into_iter().map(|(key, value)| StorageEntry { key, value }),
                    );
                    StorageDiff { address, storage_entries }
                },
            )),
            declared_contract_hashes: diff.declared_contract_hashes,
            nonces: Vec::from_iter(
                diff.nonces
                    .into_iter()
                    .map(|(contract_address, nonce)| ContractNonce { contract_address, nonce }),
            ),
        }
    }
}

/// The nonce of a StarkNet contract.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractNonce {
    pub contract_address: ContractAddress,
    pub nonce: Nonce,
}

/// A deployed contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

/// Storage differences in StarkNet.
// Invariant: Storage keys are strictly increasing. In particular, no key appears twice.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    storage_entries: Vec<StorageEntry>,
}

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}

// The StorageSerde implementation for serde_json::Value writes the length (in bytes)
// of the value. Here we serialize the whole program as one value so no need to write
// its length.
pub struct Program(serde_json::Value);
impl StorageSerde for Program {
    /// Serializes the entire program as one json value.
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        serde_json::to_writer(res, &self.0)?;
        Ok(())
    }

    /// Deserializes the entire program as one json value.
    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let value = serde_json::from_reader(bytes).ok()?;
        Some(Program(value))
    }
}
