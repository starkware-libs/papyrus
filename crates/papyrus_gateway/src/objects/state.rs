use std::collections::HashMap;

use papyrus_storage::compression_utils::{CompressionError, GzEncoded};
use papyrus_storage::{StorageSerde, ThinStateDiff};
use serde::{Deserialize, Serialize};
use starknet_api::{BlockHash, EntryPoint, EntryPointType, GlobalRoot};

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: serde_json::Value,
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

        Ok(Self {
            abi: class.abi,
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
