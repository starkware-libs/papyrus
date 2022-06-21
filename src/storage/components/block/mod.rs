mod header;
#[cfg(test)]
mod test_utils;

use crate::starknet::BlockNumber;
use std::path::Path;
use std::sync::Arc;

use crate::storage::db::open_env;
use crate::storage::db::DbError;
use crate::storage::db::DbReader;
use crate::storage::db::DbWriter;
use crate::storage::db::TableIdentifier;

pub use self::header::{HeaderStorageReader, HeaderStorageWriter};

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
pub type BlockStorageResult<V> = std::result::Result<V, BlockStorageError>;

pub struct Tables {
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

pub fn open_block_storage(
    path: &Path,
) -> BlockStorageResult<(BlockStorageReader, BlockStorageWriter)> {
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
