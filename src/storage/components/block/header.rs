#[cfg(test)]
#[path = "header_test.rs"]
mod header_test;

use crate::starknet::{BlockHash, BlockHeader, BlockNumber};
use crate::storage::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};

use super::{BlockStorageError, BlockStorageResult, BlockStorageTxn, MarkerKind, MarkersTable};

pub type BlockHashToNumberTable<'env> = TableHandle<'env, BlockHash, BlockNumber>;

pub trait HeaderStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_header_marker(&self) -> BlockStorageResult<BlockNumber>;
    fn get_block_header(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<BlockHeader>>;
    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> BlockStorageResult<Option<BlockNumber>>;
}
pub trait HeaderStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_header(
        self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> BlockStorageResult<Self>;
}
impl<'env, Mode: TransactionKind> HeaderStorageReader for BlockStorageTxn<'env, Mode> {
    fn get_header_marker(&self) -> BlockStorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table
            .get(&self.txn, &MarkerKind::Header)?
            .unwrap_or_default())
    }
    fn get_block_header(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<BlockHeader>> {
        let headers_table = self.txn.open_table(&self.tables.headers)?;
        let block_header = headers_table.get(&self.txn, &block_number)?;
        Ok(block_header)
    }
    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> BlockStorageResult<Option<BlockNumber>> {
        let block_hash_to_number_table = self.txn.open_table(&self.tables.block_hash_to_number)?;
        let block_number = block_hash_to_number_table.get(&self.txn, block_hash)?;
        Ok(block_number)
    }
}
impl<'env> HeaderStorageWriter for BlockStorageTxn<'env, RW> {
    fn append_header(
        self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> BlockStorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let headers_table = self.txn.open_table(&self.tables.headers)?;
        let block_hash_to_number_table = self.txn.open_table(&self.tables.block_hash_to_number)?;

        update_marker(&self.txn, &markers_table, block_number)?;

        // Write header.
        headers_table.insert(&self.txn, &block_number, block_header)?;

        // Write mapping.
        update_hash_mapping(
            &self.txn,
            &block_hash_to_number_table,
            block_header,
            block_number,
        )?;
        Ok(self)
    }
}

fn update_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    block_hash_to_number_table: &'env BlockHashToNumberTable<'env>,
    block_header: &BlockHeader,
    block_number: BlockNumber,
) -> Result<(), BlockStorageError> {
    let res = block_hash_to_number_table.insert(txn, &block_header.block_hash, &block_number);
    res.map_err(|err| match err {
        DbError::InnerDbError(libmdbx::Error::KeyExist) => {
            BlockStorageError::BlockHashAlreadyExists {
                block_hash: block_header.block_hash,
                block_number,
            }
        }
        err => err.into(),
    })?;
    Ok(())
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    // Make sure marker is consistent.
    let header_marker = markers_table
        .get(txn, &MarkerKind::Header)?
        .unwrap_or_default();
    if header_marker != block_number {
        return Err(BlockStorageError::MarkerMismatch {
            expected: header_marker,
            found: block_number,
        });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Header, &block_number.next())?;
    Ok(())
}
