#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::collections::HashMap;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::core::PatriciaKey;
use super::{
    BlockNumber, ClassHash, ContractAddress, EntryPointSelector, Nonce, StarkFelt, StarkHash,
    StarknetApiError,
};

/// The sequential numbering of the states between blocks in StarkNet.
// Example:
// States: S0       S1       S2
// Blocks      B0->     B1->
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StateNumber(BlockNumber);
impl StateNumber {
    // The state at the beginning of the block.
    pub fn right_before_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number)
    }
    // The state at the end of the block.
    pub fn right_after_block(block_number: BlockNumber) -> StateNumber {
        StateNumber(block_number.next())
    }
    pub fn is_before(&self, block_number: BlockNumber) -> bool {
        self.0 <= block_number
    }
    pub fn is_after(&self, block_number: BlockNumber) -> bool {
        self.0 > block_number
    }
    pub fn block_after(&self) -> BlockNumber {
        self.0
    }
}

/// The offset of an entry point in StarkNet.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointOffset(pub StarkFelt);

/// An entry point of a contract in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EntryPoint {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
}

/// A program corresponding to a contract class in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Program {
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub builtins: serde_json::Value,
    pub data: serde_json::Value,
    pub debug_info: serde_json::Value,
    pub hints: serde_json::Value,
    pub identifiers: serde_json::Value,
    pub main_scope: serde_json::Value,
    pub prime: serde_json::Value,
    pub reference_manager: serde_json::Value,
}

/// An entry point type of a contract in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    /// A constructor entry point.
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    /// An external4 entry point.
    #[serde(rename = "EXTERNAL")]
    External,
    /// An L1 handler entry point.
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

impl Default for EntryPointType {
    fn default() -> Self {
        EntryPointType::L1Handler
    }
}

/// A contract class abi entry in StarkNet.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ContractClassAbiEntry {
    /// An event abi entry.
    Event(EventAbiEntry),
    /// A function abi entry.
    Function(FunctionAbiEntry),
    /// An l1-handler abi entry.
    L1Handler(L1HandlerAbiEntry),
    /// A struct abi entry.
    Struct(StructAbiEntry),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct TypedParameter {
    pub name: String,
    pub r#type: String,
}

/// An event abi entry in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EventAbiEntry {
    pub name: String,
    pub keys: Vec<TypedParameter>,
    pub data: Vec<TypedParameter>,
}

/// A function abi entry in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FunctionAbiEntry {
    pub name: String,
    pub inputs: Vec<TypedParameter>,
    pub outputs: Vec<TypedParameter>,
}

/// A function abi entry in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct L1HandlerAbiEntry {
    pub name: String,
    pub inputs: Vec<TypedParameter>,
    pub outputs: Vec<TypedParameter>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructMember {
    #[serde(flatten)]
    pub param: TypedParameter,
    pub offset: usize,
}

/// A struct abi entry in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructAbiEntry {
    pub name: String,
    pub size: usize,
    pub members: Vec<StructMember>,
}

/// A contract class in StarkNet.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: Option<Vec<ContractClassAbiEntry>>,
    pub program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

// TODO(anatg): Consider replacing this with ThinStateDiff (that is, remove ContractClass)
// and append contract classes to the storage separately.
// Invariant: Addresses are strictly increasing.
/// The differences between two states in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StateDiff {
    deployed_contracts: Vec<DeployedContract>,
    storage_diffs: Vec<StorageDiff>,
    declared_classes: Vec<DeclaredContract>,
    nonces: Vec<ContractNonce>,
}

type StateDiffAsTuple =
    (Vec<DeployedContract>, Vec<StorageDiff>, Vec<DeclaredContract>, Vec<ContractNonce>);

fn is_unique<T, B, F>(sorted: &[T], f: F) -> bool
where
    F: Fn(&T) -> &B,
    B: PartialEq,
{
    sorted.windows(2).all(|w| f(&w[0]) != f(&w[1]))
}

impl StateDiff {
    pub fn new(
        mut deployed_contracts: Vec<DeployedContract>,
        mut storage_diffs: Vec<StorageDiff>,
        mut declared_contracts: Vec<DeclaredContract>,
        mut nonces: Vec<ContractNonce>,
    ) -> Result<Self, StarknetApiError> {
        deployed_contracts.sort_by_key(|dc| dc.address);
        storage_diffs.sort_by_key(|sd| sd.address);
        declared_contracts.sort_by_key(|dc| dc.class_hash);
        nonces.sort_by_key(|n| n.contract_address);

        if !is_unique(&deployed_contracts, |dc| &dc.address) {
            return Err(StarknetApiError::DuplicateInStateDiff {
                object: String::from("deployed_contracts"),
            });
        }

        if !is_unique(&declared_contracts, |dc| dc) {
            return Err(StarknetApiError::DuplicateInStateDiff {
                object: String::from("declared_contracts"),
            });
        }

        Ok(Self { deployed_contracts, storage_diffs, declared_classes: declared_contracts, nonces })
    }

    pub fn destruct(self) -> StateDiffAsTuple {
        (self.deployed_contracts, self.storage_diffs, self.declared_classes, self.nonces)
    }
    pub fn deployed_contracts(&self) -> &Vec<DeployedContract> {
        &self.deployed_contracts
    }
    pub fn storage_diffs(&self) -> &Vec<StorageDiff> {
        &self.storage_diffs
    }
    pub fn declared_contracts(&self) -> &Vec<DeclaredContract> {
        &self.declared_classes
    }
    pub fn nonces(&self) -> &Vec<ContractNonce> {
        &self.nonces
    }
}

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

/// A declared contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclaredContract {
    pub class_hash: ClassHash,
    pub contract_class: ContractClass,
}

// Invariant: Addresses are strictly increasing. In particular, no address appears twice.
// TODO(spapini): Enforce the invariant.
/// Storage differences in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageDiff {
    pub address: ContractAddress,
    pub storage_entries: Vec<StorageEntry>,
}

// TODO: Invariant: this is in range.
// TODO(spapini): Enforce the invariant.
/// A storage key in a StarkNet contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageKey(PatriciaKey);

impl TryFrom<StarkHash> for StorageKey {
    type Error = StarknetApiError;
    fn try_from(val: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::new(val)?))
    }
}

impl StorageKey {
    pub fn key(&self) -> &PatriciaKey {
        &self.0
    }
}

/// A storage entry in a StarkNet contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}
