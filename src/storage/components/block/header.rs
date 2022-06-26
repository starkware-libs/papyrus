#[cfg(test)]
#[path = "header_test.rs"]
mod header_test;

use libmdbx::RW;

use crate::{
    starknet::{BlockHash, BlockHeader, BlockNumber},
    storage::db::{DbError, DbTransaction, TableHandle},
};

use super::{BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter};

// Constants.
const HEADER_MARKER_KEY: &[u8] = b"header";

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
pub trait HeaderStorageWriter {
    fn append_header(
        &mut self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> BlockStorageResult<()>;
}
impl HeaderStorageReader for BlockStorageReader {
    fn get_header_marker(&self) -> BlockStorageResult<BlockNumber> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(txn
            .get::<BlockNumber>(&markers_table, HEADER_MARKER_KEY)?
            .unwrap_or_default())
    }
    fn get_block_header(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<BlockHeader>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let headers_table = txn.open_table(&self.tables.headers)?;
        let block_header =
            txn.get::<BlockHeader>(&headers_table, &bincode::serialize(&block_number).unwrap())?;
        Ok(block_header)
    }
    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> BlockStorageResult<Option<BlockNumber>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let block_hash_to_number_table = txn.open_table(&self.tables.block_hash_to_number)?;
        let block_number = txn.get::<BlockNumber>(
            &block_hash_to_number_table,
            &bincode::serialize(&block_hash).unwrap(),
        )?;
        Ok(block_number)
    }
}
impl HeaderStorageWriter for BlockStorageWriter {
    fn append_header(
        &mut self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> BlockStorageResult<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let headers_table = txn.open_table(&self.tables.headers)?;
        let block_hash_to_number_table = txn.open_table(&self.tables.block_hash_to_number)?;

        update_marker(&txn, &markers_table, block_number)?;

        // Write header.
        txn.insert(
            &headers_table,
            &bincode::serialize(&block_number).unwrap(),
            block_header,
        )?;

        // Write mapping.
        update_hash_mapping(&txn, block_hash_to_number_table, block_header, block_number)?;

        txn.commit()?;
        Ok(())
    }
}

fn update_hash_mapping(
    txn: &DbTransaction<'_, RW>,
    block_hash_to_number_table: TableHandle<'_>,
    block_header: &BlockHeader,
    block_number: BlockNumber,
) -> Result<(), BlockStorageError> {
    let res = txn.insert(
        &block_hash_to_number_table,
        &bincode::serialize(&block_header.block_hash).unwrap(),
        &block_number,
    );
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

fn update_marker(
    txn: &DbTransaction<'_, RW>,
    markers_table: &TableHandle<'_>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    // Make sure marker is consistent.
    let header_marker = txn
        .get::<BlockNumber>(markers_table, HEADER_MARKER_KEY)?
        .unwrap_or_default();
    if header_marker != block_number {
        return Err(BlockStorageError::MarkerMismatch {
            expected: header_marker,
            found: block_number,
        });
    };

    // Advance marker.
    txn.upsert(markers_table, HEADER_MARKER_KEY, &block_number.next())?;
    Ok(())
}
