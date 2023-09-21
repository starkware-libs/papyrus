use std::collections::HashMap;

use papyrus_storage::compression_utils::serialize_and_compress;
use papyrus_storage::db::serialization::{StorageSerde, StorageSerdeError};
use serde::{Deserialize, Serialize};
use starknet_api::core::EntryPointSelector;
use starknet_api::deprecated_contract_class::{ContractClassAbiEntry, EntryPointType};

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EntryPointInteger {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EntryPointOffset(pub usize);

impl From<starknet_api::deprecated_contract_class::EntryPointOffset> for EntryPointOffset {
    fn from(entry_point_offset: starknet_api::deprecated_contract_class::EntryPointOffset) -> Self {
        EntryPointOffset(entry_point_offset.0)
    }
}

impl From<starknet_api::deprecated_contract_class::EntryPoint> for EntryPointInteger {
    fn from(entry_point: starknet_api::deprecated_contract_class::EntryPoint) -> Self {
        Self { selector: entry_point.selector, offset: entry_point.offset.into() }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: Vec<ContractClassAbiEntry>,
    /// A base64 encoding of the gzip-compressed JSON representation of program.
    pub program: String,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPointInteger>>,
}

impl TryFrom<starknet_api::deprecated_contract_class::ContractClass> for ContractClass {
    type Error = StorageSerdeError;
    fn try_from(
        class: starknet_api::deprecated_contract_class::ContractClass,
    ) -> Result<Self, Self::Error> {
        let mut program_value = serde_json::to_value(&class.program)?;
        // Remove the 'attributes' key if it is null.
        if class.program.attributes == serde_json::value::Value::Null {
            program_value.as_object_mut().expect("Expecting json object").remove("attributes");
        }
        // Remove the 'compiler_version' key if it is null.
        if class.program.compiler_version == serde_json::value::Value::Null {
            program_value
                .as_object_mut()
                .expect("Expecting json object")
                .remove("compiler_version");
        }

        let abi = class.abi.unwrap_or(vec![]);

        Ok(Self {
            abi,
            program: base64::encode(serialize_and_compress(&Program(program_value))?),
            entry_points_by_type: class
                .entry_points_by_type
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().map(Into::<EntryPointInteger>::into).collect()))
                .collect(),
        })
    }
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
