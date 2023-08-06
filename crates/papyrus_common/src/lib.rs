use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};

/// Represents the syncing status of the node.
#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq)]
pub enum SyncingState {
    Synced,
    SyncStatus(SyncStatus),
}

impl serde::Serialize for SyncingState {
    // Serializes Synced variant into false (not syncing), and SyncStatus into its content.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Synced => serializer.serialize_bool(false),
            Self::SyncStatus(sync_status) => sync_status.serialize(serializer),
        }
    }
}

impl Default for SyncingState {
    fn default() -> Self {
        Self::SyncStatus(SyncStatus::default())
    }
}

// TODO(yoav): Add a test that verifies that the serialization conforms to the spec.

/// The status of the synchronization progress. The hash and the number of:
/// * the block from which the synchronization started,
/// * the currently syncing block,
/// * the highest known block.
#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SyncStatus {
    pub starting_block_hash: BlockHash,
    pub starting_block_num: BlockNumber,
    pub current_block_hash: BlockHash,
    pub current_block_num: BlockNumber,
    pub highest_block_hash: BlockHash,
    pub highest_block_num: BlockNumber,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}
