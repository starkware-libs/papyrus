use papyrus_common::BlockHashAndNumber;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageReader, StorageResult};
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

pub(crate) fn get_last_synced_block(
    storage_reader: StorageReader,
) -> StorageResult<BlockHashAndNumber> {
    let txn = storage_reader.begin_ro_txn()?;
    let Some(block_number) = txn.get_compiled_class_marker()?.prev() else {
        return Ok(BlockHashAndNumber::default());
    };
    let block_hash =
        txn.get_block_header(block_number)?.expect("No header for last compiled class").block_hash;
    Ok(BlockHashAndNumber { block_hash, block_number })
}
