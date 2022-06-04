#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::path::Path;
use std::sync::Arc;

use crate::{
    starknet::{BlockHeader, BlockNumber},
    storage::db::{open_env, DbError, DbReader, DbWriter, TableIdentifier},
};

#[derive(thiserror::Error, Debug)]
pub enum BlockStorageError {
    #[error(transparent)]
    InnerError(#[from] DbError),
    #[error("Marker mismatch (expected {expected:?}, found {found:?}).")]
    MarkerMismatch {
        expected: BlockNumber,
        found: BlockNumber,
    },
}
pub type Result<V> = std::result::Result<V, BlockStorageError>;

// Constants.
const BLOCK_MARKER_KEY: &[u8] = b"block_number";

struct Tables {
    markers: TableIdentifier,
    headers: TableIdentifier,
}
#[derive(Clone)]
pub struct BlockStorageReader {
    db_reader: DbReader,
    tables: Arc<Tables>,
}
pub struct BlockStorageWriter {
    db_writer: DbWriter,
    tables: Arc<Tables>,
}

#[allow(dead_code)]
pub fn open_block_storage(path: &Path) -> Result<(BlockStorageReader, BlockStorageWriter)> {
    let (db_reader, mut db_writer) = open_env(path)?;
    let tables = Arc::new(Tables {
        markers: db_writer.create_table("markers")?,
        headers: db_writer.create_table("headers")?,
    });
    let reader = BlockStorageReader {
        db_reader,
        tables: tables.clone(),
    };
    let writer = BlockStorageWriter { db_writer, tables };
    Ok((reader, writer))
}

#[allow(dead_code)]
impl BlockStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    pub fn get_header_marker(&self) -> Result<BlockNumber> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(txn
            .get::<BlockNumber>(&markers_table, BLOCK_MARKER_KEY)?
            .unwrap_or_default())
    }
    pub fn get_block_header(&self, block_number: BlockNumber) -> Result<Option<BlockHeader>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let headers_table = txn.open_table(&self.tables.headers)?;
        let block_header =
            txn.get::<BlockHeader>(&headers_table, &bincode::serialize(&block_number).unwrap())?;
        Ok(block_header)
    }
}
impl BlockStorageWriter {
    pub fn append_header(
        &mut self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> Result<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let headers_table = txn.open_table(&self.tables.headers)?;

        // Make sure marker is consistent.
        let header_marker = txn
            .get::<BlockNumber>(&markers_table, BLOCK_MARKER_KEY)?
            .unwrap_or_default();
        if header_marker != block_number {
            return Err(BlockStorageError::MarkerMismatch {
                expected: header_marker,
                found: block_number,
            });
        };

        // Advance marker.
        txn.upsert(&markers_table, BLOCK_MARKER_KEY, &block_number.next())?;

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
