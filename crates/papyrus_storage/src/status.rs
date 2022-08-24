#[cfg(test)]
#[path = "status_test.rs"]
mod status_test;

use starknet_api::{BlockNumber, BlockStatus};

use super::db::{DbTransaction, TransactionKind, RW};
use super::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

pub trait StatusStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_status_marker(&self) -> StorageResult<BlockNumber>;
    fn get_block_status(&self, block_number: BlockNumber) -> StorageResult<Option<BlockStatus>>;
}
pub trait StatusStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_block_status(
        self,
        block_number: BlockNumber,
        block_status: &BlockStatus,
    ) -> StorageResult<Self>;
}
impl<'env, Mode: TransactionKind> StatusStorageReader for StorageTxn<'env, Mode> {
    fn get_status_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Status)?.unwrap_or_default())
    }
    fn get_block_status(&self, block_number: BlockNumber) -> StorageResult<Option<BlockStatus>> {
        let statuses_table = self.txn.open_table(&self.tables.statuses)?;
        let block_status = statuses_table.get(&self.txn, &block_number)?;
        Ok(block_status)
    }
}
impl<'env> StatusStorageWriter for StorageTxn<'env, RW> {
    fn append_block_status(
        self,
        block_number: BlockNumber,
        block_status: &BlockStatus,
    ) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let statuses_table = self.txn.open_table(&self.tables.statuses)?;

        update_marker(&self.txn, &markers_table, block_number)?;
        // Write status.
        statuses_table.insert(&self.txn, &block_number, block_status)?;

        Ok(self)
    }
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    // Make sure marker is consistent.
    let status_marker = markers_table.get(txn, &MarkerKind::Status)?.unwrap_or_default();
    if status_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: status_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Status, &block_number.next())?;
    Ok(())
}
