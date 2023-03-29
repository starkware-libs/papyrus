use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::db::RW;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::OmmerStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use tracing::info;

use crate::data::{BlockSyncData, SyncDataTrait};
use crate::sources::CentralSourceTrait;
use crate::sync::SyncExtensionTrait;
use crate::{StateSyncError, StateSyncResult, SyncConfig};

pub struct BlockSync {}
#[async_trait]
impl<T: CentralSourceTrait + Sync + Send + 'static> SyncExtensionTrait<T, BlockSyncData>
    for BlockSync
{
    fn get_from(reader: &StorageReader) -> Result<BlockNumber, StateSyncError> {
        Ok(reader.begin_ro_txn()?.get_header_marker()?)
    }

    async fn get_range(
        reader: StorageReader,
        source: Arc<T>,
    ) -> Result<(BlockNumber, BlockNumber), StateSyncError> {
        Ok((reader.begin_ro_txn()?.get_header_marker()?, source.get_block_marker().await?))
    }

    fn get_sleep_duration(config: SyncConfig) -> Duration {
        config.block_propagation_sleep_duration
    }

    fn store(txn: StorageTxn<'_, RW>, data: BlockSyncData) -> StateSyncResult {
        let block_number = data.block_number();
        Ok(txn
            .append_header(block_number, &data.block.header)?
            .append_body(block_number, data.block.body)?
            .commit()?)
    }

    fn should_store(reader: &StorageReader, data: &BlockSyncData) -> Result<bool, StateSyncError> {
        let prev_block_number = match data.block_number().prev() {
            None => return Ok(true),
            Some(bn) => bn,
        };

        if let Some(prev_header) = reader.begin_ro_txn()?.get_block_header(prev_block_number)? {
            // Compares the block's parent hash to the stored block.
            if prev_header.block_hash == data.block.header.parent_hash {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn revert_if_necessary(mut txn: StorageTxn<'_, RW>, data: &BlockSyncData) -> StateSyncResult {
        if let Some(block_number) = data.block_number().prev() {
            info!("Reverting block {}.", block_number);
            let res = txn.revert_header(block_number)?;
            txn = res.0;
            if let Some(header) = res.1 {
                txn = txn.insert_ommer_header(header.block_hash, &header)?;
                let res = txn.revert_body(block_number)?;
                txn = res.0;
                if let Some((transactions, transaction_outputs, events)) = res.1 {
                    txn = txn.insert_ommer_body(
                        header.block_hash,
                        &transactions,
                        &transaction_outputs,
                        events.as_slice(),
                    )?;
                }

                let res = txn.revert_state_diff(block_number)?;
                txn = res.0;
                if let Some((thin_state_diff, declared_classes)) = res.1 {
                    txn = txn.insert_ommer_state_diff(
                        header.block_hash,
                        &thin_state_diff,
                        &declared_classes,
                    )?;
                }
            }
            txn.commit()?;
        }
        Ok(())
    }
}
