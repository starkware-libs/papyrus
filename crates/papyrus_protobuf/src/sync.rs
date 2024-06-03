use std::fmt::Debug;

use indexmap::IndexMap;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StorageKey, ThinStateDiff};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDiff {
    pub contract_address: ContractAddress,
    // Has value only if the contract was deployed or replaced in this block.
    pub class_hash: Option<ClassHash>,
    // Has value only if the nonce was updated in this block.
    pub nonce: Option<Nonce>,
    pub storage_diffs: IndexMap<StorageKey, StarkFelt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredClass {
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedDeclaredClass {
    pub class_hash: ClassHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDiffChunk {
    ContractDiff(ContractDiff),
    DeclaredClass(DeclaredClass),
    DeprecatedDeclaredClass(DeprecatedDeclaredClass),
}

pub struct StateDiffChunkVec(pub Vec<StateDiffChunk>);

impl From<ThinStateDiff> for StateDiffChunkVec {
    fn from(thin_state_diff: ThinStateDiff) -> Self {
        let mut state_diff_chunks = Vec::new();

        for (contract_address, class_hash) in thin_state_diff.deployed_contracts.iter() {
            state_diff_chunks.push(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: *contract_address,
                class_hash: Some(*class_hash),
                nonce: thin_state_diff.nonces.get(contract_address).cloned(),
                storage_diffs: thin_state_diff
                    .storage_diffs
                    .get(contract_address)
                    .cloned()
                    .unwrap_or_default(),
            }));
        }

        for (contract_address, class_hash) in thin_state_diff.replaced_classes.iter() {
            state_diff_chunks.push(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: *contract_address,
                class_hash: Some(*class_hash),
                nonce: thin_state_diff.nonces.get(contract_address).cloned(),
                storage_diffs: thin_state_diff
                    .storage_diffs
                    .get(contract_address)
                    .cloned()
                    .unwrap_or_default(),
            }));
        }

        for (class_hash, compiled_class_hash) in thin_state_diff.declared_classes.iter() {
            state_diff_chunks.push(StateDiffChunk::DeclaredClass(DeclaredClass {
                class_hash: *class_hash,
                compiled_class_hash: *compiled_class_hash,
            }));
        }

        for class_hash in thin_state_diff.deprecated_declared_classes.iter() {
            state_diff_chunks.push(StateDiffChunk::DeprecatedDeclaredClass(
                DeprecatedDeclaredClass { class_hash: *class_hash },
            ));
        }

        StateDiffChunkVec(state_diff_chunks)
    }
}
