use std::collections::HashMap;

use papyrus_storage::encoding_utils::{Base64Encoded, EncodingDecodingError, GzEncoded};
use papyrus_storage::ThinStateDiff;
use serde::{Deserialize, Serialize};
use starknet_api::{BlockHash, EntryPoint, EntryPointType, GlobalRoot, Program};

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: serde_json::Value,
    pub program: Base64Encoded<GzEncoded<Program>>,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl TryFrom<starknet_api::ContractClass> for ContractClass {
    type Error = EncodingDecodingError;
    fn try_from(class: starknet_api::ContractClass) -> Result<Self, Self::Error> {
        Ok(Self {
            abi: class.abi,
            program: Base64Encoded::encode(GzEncoded::encode(class.program)?)?,
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
