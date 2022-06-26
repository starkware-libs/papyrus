mod header;
mod state;
#[cfg(test)]
mod test_utils;

use crate::starknet::{BlockHash, BlockNumber};
use crate::storage::db::DbConfig;
use std::sync::Arc;

use crate::storage::db::{open_env, DbError, DbReader, DbWriter, TableIdentifier};

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
    #[error(
        "Block hash {block_hash:?} already exists, when adding block number {block_number:?}."
    )]
    BlockHashAlreadyExists {
        block_hash: BlockHash,
        block_number: BlockNumber,
    },
}
pub type BlockStorageResult<V> = std::result::Result<V, BlockStorageError>;

pub struct Tables {
    markers: TableIdentifier,
    headers: TableIdentifier,
    block_hash_to_number: TableIdentifier,
    state_diffs: TableIdentifier,
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
    db_config: DbConfig,
) -> BlockStorageResult<(BlockStorageReader, BlockStorageWriter)> {
    let (db_reader, mut db_writer) = open_env(db_config)?;
    let tables = Arc::new(Tables {
        markers: db_writer.create_table("markers")?,
        headers: db_writer.create_table("headers")?,
        block_hash_to_number: db_writer.create_table("block_hash_to_number")?,
        state_diffs: db_writer.create_table("state_diffs")?,
    });
    let reader = BlockStorageReader {
        db_reader,
        tables: tables.clone(),
    };
    let writer = BlockStorageWriter { db_writer, tables };
    Ok((reader, writer))
}
