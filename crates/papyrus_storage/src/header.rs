//! Interface for handling data related to Starknet [block headers](https://docs.rs/starknet_api/latest/starknet_api/block/struct.BlockHeader.html).
//!
//! The block header is the part of the block that contains metadata about the block.
//! Import [`HeaderStorageReader`] and [`HeaderStorageWriter`] to read and write data related
//! to the block headers using a [`StorageTxn`].
//! # Example
//! ```
//! use papyrus_storage::open_storage;
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! use starknet_api::block::{Block, BlockNumber};
//! use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId("SN_MAIN".to_owned()),
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! # };
//! let block = Block::default();
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let (reader, mut writer) = open_storage(storage_config)?;
//! writer
//!     .begin_rw_txn()?                                // Start a RW transaction.
//!     .append_header(BlockNumber(0), &block.header)?  // Appending a block body will fail without matching header.
//!     .commit()?;
//!
//! let header = reader.begin_ro_txn()?.get_block_header(BlockNumber(0))?;
//! assert_eq!(header, Some(block.header));
//! # Ok::<(), papyrus_storage::StorageError>(())
//! ```

#[cfg(test)]
#[path = "header_test.rs"]
mod header_test;

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use tracing::debug;

use crate::db::{DbTransaction, TableHandle, TransactionKind, RW};
use crate::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

type BlockHashToNumberTable<'env> = TableHandle<'env, BlockHash, BlockNumber>;

/// Interface for reading data related to the block headers.
pub trait HeaderStorageReader {
    /// The block marker is the first block number that doesn't exist yet.
    fn get_header_marker(&self) -> StorageResult<BlockNumber>;
    /// Returns the header of the block with the given number.
    fn get_block_header(&self, block_number: BlockNumber) -> StorageResult<Option<BlockHeader>>;

    /// Returns the block number of the block with the given hash.
    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>>;

    /// Returns the Starknet version at the given block number.
    fn get_starknet_version(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StarknetVersion>>;
}

/// Interface for writing data related to the block headers.
pub trait HeaderStorageWriter
where
    Self: Sized,
{
    /// Appends a header to the storage.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_header(
        self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> StorageResult<Self>;

    /// Update the starknet version if needed.
    fn update_starknet_version(
        self,
        block_number: &BlockNumber,
        starknet_version: &StarknetVersion,
    ) -> StorageResult<Self>;

    /// Removes a block header from the storage and returns the removed data.
    fn revert_header(self, block_number: BlockNumber)
    -> StorageResult<(Self, Option<BlockHeader>)>;
}

/// A version of the Starknet protocol used when creating a block.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StarknetVersion(pub String);

impl<'env, Mode: TransactionKind> HeaderStorageReader for StorageTxn<'env, Mode> {
    fn get_header_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Header)?.unwrap_or_default())
    }

    fn get_block_header(&self, block_number: BlockNumber) -> StorageResult<Option<BlockHeader>> {
        let headers_table = self.open_table(&self.tables.headers)?;
        let block_header = headers_table.get(&self.txn, &block_number)?;
        Ok(block_header)
    }

    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>> {
        let block_hash_to_number_table = self.open_table(&self.tables.block_hash_to_number)?;
        let block_number = block_hash_to_number_table.get(&self.txn, block_hash)?;
        Ok(block_number)
    }

    fn get_starknet_version(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StarknetVersion>> {
        if block_number >= self.get_header_marker()? {
            return Ok(None);
        }

        let starknet_version_table = self.open_table(&self.tables.starknet_version)?;
        let mut cursor = starknet_version_table.cursor(&self.txn)?;
        cursor.lower_bound(&block_number.next())?;
        let res = cursor.prev()?;

        match res {
            Some((_block_number, starknet_version)) => Ok(Some(starknet_version)),
            None => unreachable!(
                "Since block_number >= self.get_header_marker(), starknet_version_table should \
                 have at least a single mapping."
            ),
        }
    }
}

impl<'env> HeaderStorageWriter for StorageTxn<'env, RW> {
    fn append_header(
        self,
        block_number: BlockNumber,
        block_header: &BlockHeader,
    ) -> StorageResult<Self> {
        let markers_table = self.open_table(&self.tables.markers)?;
        let headers_table = self.open_table(&self.tables.headers)?;
        let block_hash_to_number_table = self.open_table(&self.tables.block_hash_to_number)?;

        update_marker(&self.txn, &markers_table, block_number)?;

        // Write header.
        headers_table.insert(&self.txn, &block_number, block_header)?;

        // Write mapping.
        update_hash_mapping(&self.txn, &block_hash_to_number_table, block_header, block_number)?;

        Ok(self)
    }

    fn update_starknet_version(
        self,
        block_number: &BlockNumber,
        starknet_version: &StarknetVersion,
    ) -> StorageResult<Self> {
        let starknet_version_table = self.open_table(&self.tables.starknet_version)?;
        let mut cursor = starknet_version_table.cursor(&self.txn)?;
        cursor.lower_bound(block_number)?;
        let res = cursor.prev()?;

        match res {
            Some((_block_number, last_starknet_version))
                if last_starknet_version == *starknet_version => {}
            _ => starknet_version_table.insert(&self.txn, block_number, starknet_version)?,
        }
        Ok(self)
    }

    fn revert_header(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<BlockHeader>)> {
        let markers_table = self.open_table(&self.tables.markers)?;
        let headers_table = self.open_table(&self.tables.headers)?;
        let block_hash_to_number_table = self.open_table(&self.tables.block_hash_to_number)?;
        let starknet_version_table = self.open_table(&self.tables.starknet_version)?;

        // Assert that header marker equals the reverted block number + 1
        let current_header_marker = self.get_header_marker()?;

        // Reverts only the last header.
        if current_header_marker != block_number.next() {
            debug!(
                "Attempt to revert a non-existing / old header of block {}. Returning without an \
                 action.",
                block_number
            );
            return Ok((self, None));
        }

        let reverted_header = headers_table
            .get(&self.txn, &block_number)?
            .expect("Missing header for block {block_number}.");
        markers_table.upsert(&self.txn, &MarkerKind::Header, &block_number)?;
        headers_table.delete(&self.txn, &block_number)?;
        block_hash_to_number_table.delete(&self.txn, &reverted_header.block_hash)?;

        // Revert starknet version.
        starknet_version_table.delete(&self.txn, &block_number)?;

        Ok((self, Some(reverted_header)))
    }
}

fn update_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    block_hash_to_number_table: &'env BlockHashToNumberTable<'env>,
    block_header: &BlockHeader,
    block_number: BlockNumber,
) -> Result<(), StorageError> {
    block_hash_to_number_table.insert(txn, &block_header.block_hash, &block_number)?;
    Ok(())
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    // Make sure marker is consistent.
    let header_marker = markers_table.get(txn, &MarkerKind::Header)?.unwrap_or_default();
    if header_marker != block_number
        && block_number.0
            > std::env::var("PAPYRUS_START_BLOCK")
                .expect("Set the variable PAPYRUS_START_BLOCK to a number")
                .parse::<u64>()
                .expect("Set the variable PAPYRUS_START_BLOCK to a number")
    {
        return Err(StorageError::MarkerMismatch { expected: header_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Header, &block_number.next())?;
    Ok(())
}

impl Default for StarknetVersion {
    fn default() -> Self {
        Self("0.0.0".to_string())
    }
}

impl Display for StarknetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
