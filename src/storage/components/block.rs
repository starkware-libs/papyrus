#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::{path::Path, sync::Arc};

use crate::{
    starknet::BlockNumber,
    storage::db::{open_env, DbError, DbReader, DbWriter, TableIdentifier},
};

#[derive(thiserror::Error, Debug)]
pub enum BlockStorageError {
    #[error(transparent)]
    InnerError(#[from] DbError),
}

// Constants.
struct Tables {
    markers: TableIdentifier,
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
pub fn open_block_storage(
    path: &Path,
) -> Result<(BlockStorageReader, BlockStorageWriter), BlockStorageError> {
    let (db_reader, mut db_writer) = open_env(path)?;
    let tables = Arc::new(Tables {
        markers: db_writer.create_table("markers")?,
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
    pub fn get_block_number_marker(&self) -> Result<BlockNumber, BlockStorageError> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(txn
            .get::<BlockNumber>(&markers_table, b"block_number")?
            .unwrap_or_default())
    }
}
#[allow(dead_code)]
impl BlockStorageWriter {
    pub fn set_block_number_marker(
        &mut self,
        block_number: BlockNumber,
    ) -> Result<(), BlockStorageError> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        txn.upsert(&markers_table, b"block_number", &block_number)?;
        txn.commit()?;
        Ok(())
    }
}
