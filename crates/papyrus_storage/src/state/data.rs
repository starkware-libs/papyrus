use serde::{Deserialize, Serialize};
use starknet_api::{BlockNumber, ClassHash, ContractNonce, DeployedContract, StorageDiff};

// Data structs that are serialized into the database.

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct IndexedDeployedContract {
    pub block_number: BlockNumber,
    pub class_hash: ClassHash,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct IndexedDeclaredContract {
    pub block_number: BlockNumber,
    pub contract_class: Vec<u8>,
}

// Invariant: Addresses are strictly increasing.
// TODO(spapini): Enforce the invariant.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinStateDiff {
    pub deployed_contracts: Vec<DeployedContract>,
    pub storage_diffs: Vec<StorageDiff>,
    pub declared_classes: Vec<ClassHash>,
    pub nonces: Vec<ContractNonce>,
}
