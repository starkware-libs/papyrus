use starknet_api::block::BlockHash;
use starknet_api::transaction::Transaction;
use starknet_types_core::felt::Felt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Proposal {
    pub height: u64,
    pub proposer: Felt,
    pub transactions: Vec<Transaction>,
    pub block_hash: BlockHash,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConsensusMessage {
    Proposal(Proposal),
}
