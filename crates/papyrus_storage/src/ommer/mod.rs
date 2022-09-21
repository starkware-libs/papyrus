use starknet_api::{BlockHash, BlockHeader, BlockBody};

use crate::db::{RW, DbTransaction, TableHandle};
use crate::{StorageResult, StorageTxn, ThinStateDiff};

#[cfg(test)]
#[path = "ommer_test.rs"]
mod ommer_test;

pub trait OmmerStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn insert_ommer_block(
        self,
        block_hash: BlockHash,
        block_header: &BlockHeader,
        block_body: &BlockBody,
        state_diff: &ThinStateDiff,
    ) -> StorageResult<Self>;
}

impl<'env> OmmerStorageWriter for StorageTxn<'env, RW> {
    fn insert_ommer_block(
        self,
        block_hash: BlockHash,
        block_header: &BlockHeader,
        _block_body: &BlockBody,
        _state_diff: &ThinStateDiff,
    ) -> StorageResult<Self> {
        let headers_table = self.txn.open_table(&self.tables.ommer_headers)?;

        insert_ommer_header(&self.txn, &headers_table, block_hash, block_header)?;

        // TODO(yair): insert body and state_diff
        Ok(self)
    }
}

type HeadersTable<'env> = TableHandle<'env, BlockHash, BlockHeader>;
fn insert_ommer_header<'env>(
    txn: &DbTransaction<'env, RW>,
    headers_table: &'env HeadersTable<'env>,
    block_hash: BlockHash,
    block_header: &BlockHeader,
) -> StorageResult<()>  {
    headers_table.insert(txn, &block_hash, block_header)?;
    Ok(())
}
