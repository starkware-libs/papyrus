use std::collections::HashMap;

use papyrus_storage::db::serialization::StorageSerdeError;
use serde::{Deserialize, Serialize};
use starknet_api::deprecated_contract_class::{ContractClassAbiEntry, EntryPoint, EntryPointType};

use crate::compression_utils::compress_and_encode;

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: Vec<ContractClassAbiEntry>,
    /// A base64 encoding of the gzip-compressed JSON representation of program.
    pub program: String,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
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

        let abi = class.abi.unwrap_or_default();

        Ok(Self {
            abi,
            program: compress_and_encode(program_value)?,
            entry_points_by_type: class.entry_points_by_type,
        })
    }
}
