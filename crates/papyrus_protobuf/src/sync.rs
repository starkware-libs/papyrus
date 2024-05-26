use indexmap::IndexMap;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::StorageKey;

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
pub struct SignedBlockHeader {
    pub block_header: BlockHeader,
    pub signatures: Vec<BlockSignature>,
}

// TODO(shahak): Implement conversion to/from Vec<u8>.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDiffsResponse(pub Option<StateDiffPart>);

// TODO(shahak): Implement conversion to/from protobuf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDiffPart {
    ContractDiff {
        contract_address: ContractAddress,
        // Has value only if the contract was deployed or replaced in this block.
        class_hash: Option<ClassHash>,
        // Has value only if the nonce was updated in this block.
        nonce: Option<Nonce>,
        storage_diffs: IndexMap<StorageKey, StarkFelt>,
    },
    DeclaredClass {
        class_hash: ClassHash,
        compiled_class_hash: CompiledClassHash,
    },
    DeprecatedDeclaredClass {
        class_hash: ClassHash,
    },
}
