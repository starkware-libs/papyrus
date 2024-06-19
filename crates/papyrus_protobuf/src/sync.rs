use std::fmt::Debug;

use indexmap::IndexMap;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;
#[cfg(any(feature = "testing", test))]
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, GetTestInstance};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}

/// This struct represents a query that can be sent to a peer.
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Query {
    pub start_block: BlockHashOrNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum BlockHashOrNumber {
    Hash(BlockHash),
    Number(BlockNumber),
}

impl Default for BlockHashOrNumber {
    fn default() -> Self {
        Self::Number(BlockNumber::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataOrFin<T>(pub Option<T>);

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeaderQuery(pub Query);

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateDiffQuery(pub Query);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBlockHeader {
    pub block_header: BlockHeader,
    pub signatures: Vec<BlockSignature>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ContractDiff {
    pub contract_address: ContractAddress,
    // Has value only if the contract was deployed or replaced in this block.
    pub class_hash: Option<ClassHash>,
    // Has value only if the nonce was updated in this block.
    pub nonce: Option<Nonce>,
    pub storage_diffs: IndexMap<StorageKey, Felt>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeclaredClass {
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeprecatedDeclaredClass {
    pub class_hash: ClassHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDiffChunk {
    ContractDiff(ContractDiff),
    DeclaredClass(DeclaredClass),
    DeprecatedDeclaredClass(DeprecatedDeclaredClass),
}

impl StateDiffChunk {
    pub fn len(&self) -> usize {
        match self {
            StateDiffChunk::ContractDiff(contract_diff) => {
                let mut result = contract_diff.storage_diffs.len();
                if contract_diff.class_hash.is_some() {
                    result += 1;
                }
                if contract_diff.nonce.is_some() {
                    result += 1;
                }
                result
            }
            StateDiffChunk::DeclaredClass(_) => 1,
            StateDiffChunk::DeprecatedDeclaredClass(_) => 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl Default for StateDiffChunk {
    fn default() -> Self {
        Self::ContractDiff(ContractDiff::default())
    }
}

#[cfg(any(feature = "testing", test))]
auto_impl_get_test_instance! {
    pub enum StateDiffChunk{
        ContractDiff(ContractDiff) = 0,
        DeclaredClass(DeclaredClass) = 1,
        DeprecatedDeclaredClass(DeprecatedDeclaredClass) = 2,
    }
    pub struct ContractDiff{
        pub contract_address: ContractAddress,
        pub class_hash: Option<ClassHash>,
        pub nonce: Option<Nonce>,
        pub storage_diffs: IndexMap<StorageKey, Felt>,
    }
    pub struct DeclaredClass {
        pub class_hash: ClassHash,
        pub compiled_class_hash: CompiledClassHash,
    }
    pub struct DeprecatedDeclaredClass {
        pub class_hash: ClassHash,
    }
    pub struct StateDiffQuery(pub Query);
    pub struct Query {
        pub start_block: BlockHashOrNumber,
        pub direction: Direction,
        pub limit: u64,
        pub step: u64,
    }
    pub enum BlockHashOrNumber {
        Hash(BlockHash)=0,
        Number(BlockNumber)=1,
    }
    pub enum Direction {
        Forward=0,
        Backward=1,
    }
}
