use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;

pub type DeclaredClasses = IndexMap<ClassHash, ContractClass>;
pub type DeprecatedDeclaredClasses = IndexMap<ClassHash, DeprecatedContractClass>;

/// Data structs that are serialized into the database.

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct IndexedDeprecatedContractClass {
    pub block_number: BlockNumber,
    pub contract_class: DeprecatedContractClass,
}
