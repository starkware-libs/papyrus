use serde::{Deserialize, Serialize};

#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StarkHash([u64; 4]);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddress(pub StarkHash);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHash(pub StarkHash);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct GlobalRoot(pub StarkHash);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockNumber(pub u64);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub u64);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ListCommitment {
    pub length: u64,
    pub commitment: StarkHash,
}
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionsCommitment(pub ListCommitment);
#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EventsCommitment(pub ListCommitment);

#[derive(
    Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockHeader {
    pub parent_hash: BlockHash,
    pub number: BlockNumber,
    pub root: GlobalRoot,
    pub sequencer: ContractAddress,
    pub timestamp: BlockTimestamp,
    pub transactions_commitment: TransactionsCommitment,
    pub events_commitment: EventsCommitment,
}

pub struct BlockBody {}
