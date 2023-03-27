#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_storage::db::{RO, RW};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateStorageReader, StateStorageWriter};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::state::StateDiff;

use crate::data::StateDiffSyncData;
use crate::sources::CentralSourceTrait;
use crate::sync::SyncExtensionTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct StateDiffSync {}
#[async_trait]
impl<T: CentralSourceTrait + Sync + Send + 'static> SyncExtensionTrait<T, StateDiffSyncData>
    for StateDiffSync
{
    fn get_from(reader: &StorageReader) -> Result<BlockNumber, StateSyncError> {
        Ok(reader.begin_ro_txn()?.get_state_marker()?)
    }

    async fn get_range(
        txn: StorageTxn<'_, RO>,
        _source: Arc<T>,
    ) -> Result<(BlockNumber, BlockNumber), StateSyncError> {
        Ok((txn.get_state_marker()?, txn.get_header_marker()?))
    }

    fn get_sleep_duration(_config: SyncConfig) -> Duration {
        Duration::from_millis(10)
    }

    fn store(txn: StorageTxn<'_, RW>, mut data: StateDiffSyncData) -> StateSyncResult {
        sort_state_diff(&mut data.state_diff);
        Ok(txn
            .append_state_diff(
                data.block_number,
                data.state_diff,
                data.deployed_contract_class_definitions,
            )?
            .commit()?)
    }

    fn should_store(
        reader: &StorageReader,
        data: &StateDiffSyncData,
    ) -> Result<bool, StateSyncError> {
        let txn = reader.begin_ro_txn()?;
        if let Some(storage_header) = txn.get_block_header(data.block_number)? {
            if storage_header.block_hash == data.block_hash {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn revert_if_necessary(_txn: StorageTxn<'_, RW>, _data: &StateDiffSyncData) -> StateSyncResult {
        Ok(())
    }
}

pub(crate) fn sort_state_diff(diff: &mut StateDiff) {
    diff.declared_classes.sort_unstable_keys();
    diff.deployed_contracts.sort_unstable_keys();
    diff.nonces.sort_unstable_keys();
    diff.storage_diffs.sort_unstable_keys();
    for storage_entries in diff.storage_diffs.values_mut() {
        storage_entries.sort_unstable_keys();
    }
}
