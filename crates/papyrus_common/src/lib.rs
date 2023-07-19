use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SyncingState {
    #[serde(serialize_with = "serialize_false")]
    Synced,
    SyncStatus(SyncStatus),
}

pub fn serialize_false<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_bool(false)
}

// TODO(yoav): Add a test that verifies that the serialization conforms to the spec.

/// The status of the synchronization progress. The hash and the number of:
/// * the block from which the synchronization started,
/// * the currently syncing block,
/// * the highest known block.
#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SyncStatus {
    pub starting_block_hash: BlockHash,
    pub starting_block_num: BlockNumber,
    pub current_block_hash: BlockHash,
    pub current_block_num: BlockNumber,
    pub highest_block_hash: BlockHash,
    pub highest_block_num: BlockNumber,
}
