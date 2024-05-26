use std::fmt::Debug;

use indexmap::IndexMap;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{Transaction, TransactionOutput};

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
pub struct TransactionsResponse(pub Option<(Transaction, TransactionOutput)>);

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeaderQuery(pub Query);

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateDiffQuery(pub Query);

// We have this struct in order to implement on it traits.
// TODO(shahak): Rename the protobuf message from BlockHeadersResponse to BlockHeaderResponse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeaderResponse(pub Option<SignedBlockHeader>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBlockHeader {
    pub block_header: BlockHeader,
    pub signatures: Vec<BlockSignature>,
}

// TODO(shahak): Implement conversion to/from Vec<u8>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDiffsResponse(pub Option<StateDiffChunk>);

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
