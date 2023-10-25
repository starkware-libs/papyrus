use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;

use crate::mmap_file::LocationInFile;

pub type DeclaredClasses = IndexMap<ClassHash, ContractClass>;
pub type DeprecatedDeclaredClasses = IndexMap<ClassHash, DeprecatedContractClass>;

/// Data structs that are serialized into the database.

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct IndexedDeprecatedContractClass {
    pub block_number: BlockNumber,
    pub location_in_file: LocationInFile,
}
