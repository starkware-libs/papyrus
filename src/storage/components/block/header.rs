#[cfg(test)]
#[path = "header_test.rs"]
mod header_test;

use crate::starknet::{BlockHeader, BlockNumber};

use super::{BlockStorageError, BlockStorageReader, BlockStorageWriter, Result};

// Constants.
const HEADER_MARKER_KEY: &[u8] = b"header";

pub trait HeaderStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_header_marker(&self) -> Result<BlockNumber>;
    fn get_block_header(&self, block_number: BlockNumber) -> Result<Option<BlockHeader>>;
}
pub trait HeaderStorageWriter {
    // The block number marker is the first block number that doesn't exist yet.
    fn append_header(
        &mut self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> Result<()>;
}
impl HeaderStorageReader for BlockStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_header_marker(&self) -> Result<BlockNumber> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(txn
            .get::<BlockNumber>(&markers_table, HEADER_MARKER_KEY)?
            .unwrap_or_default())
    }
    fn get_block_header(&self, block_number: BlockNumber) -> Result<Option<BlockHeader>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let headers_table = txn.open_table(&self.tables.headers)?;
        let block_header =
            txn.get::<BlockHeader>(&headers_table, &bincode::serialize(&block_number).unwrap())?;
        Ok(block_header)
    }
}
impl HeaderStorageWriter for BlockStorageWriter {
    fn append_header(
        &mut self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> Result<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let headers_table = txn.open_table(&self.tables.headers)?;

        // Make sure marker is consistent.
        let header_marker = txn
            .get::<BlockNumber>(&markers_table, HEADER_MARKER_KEY)?
            .unwrap_or_default();
        if header_marker != block_number {
            return Err(BlockStorageError::MarkerMismatch {
                expected: header_marker,
                found: block_number,
            });
        };

        // Advance marker.
        txn.upsert(&markers_table, HEADER_MARKER_KEY, &block_number.next())?;

        // Write header.
        txn.insert(
            &headers_table,
            &bincode::serialize(&block_number).unwrap(),
            block_header,
        )?;

        txn.commit()?;
        Ok(())
    }
}
