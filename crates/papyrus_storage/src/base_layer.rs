#[cfg(test)]
#[path = "base_layer_test.rs"]
mod base_layer_test;

use starknet_api::block::BlockNumber;

use crate::db::{TransactionKind, RW};
use crate::{MarkerKind, StorageResult, StorageTxn};

pub trait BaseLayerStorageReader {
    // The block number marker is the first block number that doesn't exist yet in the base layer.
    fn get_base_layer_block_marker(&self) -> StorageResult<BlockNumber>;
}

pub trait BaseLayerStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn update_base_layer_block_marker(self, block_number: &BlockNumber) -> StorageResult<Self>;

    // When reverting a block, if the base layer marker points to the block afterward, revert the
    // marker.
    fn try_revert_base_layer_marker(
        self,
        reverted_block_number: BlockNumber,
    ) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> BaseLayerStorageReader for StorageTxn<'env, Mode> {
    fn get_base_layer_block_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::BaseLayerBlock)?.unwrap_or_default())
    }
}

impl<'env> BaseLayerStorageWriter for StorageTxn<'env, RW> {
    fn update_base_layer_block_marker(self, block_number: &BlockNumber) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        markers_table.upsert(&self.txn, &MarkerKind::BaseLayerBlock, block_number)?;
        Ok(self)
    }

    fn try_revert_base_layer_marker(
        self,
        reverted_block_number: BlockNumber,
    ) -> StorageResult<Self> {
        let cur_marker = self.get_base_layer_block_marker()?;
        // Revert only if we revert a block that is the last block in the base layer we know about.
        if cur_marker == reverted_block_number.next() {
            Ok(self.update_base_layer_block_marker(&reverted_block_number)?)
        } else {
            Ok(self)
        }
    }
}
