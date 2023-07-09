#[cfg(test)]
#[path = "header_test.rs"]
mod header_test;

use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use tracing::debug;

use crate::body::StarknetVersion;
use crate::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use crate::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

type BlockHashToNumberTable<'env> = TableHandle<'env, BlockHash, BlockNumber>;

pub trait HeaderStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_header_marker(&self) -> StorageResult<BlockNumber>;

    fn get_block_header(&self, block_number: BlockNumber) -> StorageResult<Option<BlockHeader>>;

    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>>;

    /// Returns the StarkNet version at the given block number.
    fn get_starknet_version(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StarknetVersion>>;
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
        starknet_version: StarknetVersion,
    ) -> StorageResult<Self>;

    fn revert_header(self, block_number: BlockNumber)
    -> StorageResult<(Self, Option<BlockHeader>)>;
}

impl<'env, Mode: TransactionKind> HeaderStorageReader for StorageTxn<'env, Mode> {
    fn get_header_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Header)?.unwrap_or_default())
    }

    fn get_block_header(&self, block_number: BlockNumber) -> StorageResult<Option<BlockHeader>> {
        let headers_table = self.txn.open_table(&self.tables.headers)?;
        let block_header = headers_table.get(&self.txn, &block_number)?;
        Ok(block_header)
    }

    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>> {
        let block_hash_to_number_table = self.txn.open_table(&self.tables.block_hash_to_number)?;
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

        let starknet_version_table = self.txn.open_table(&self.tables.starknet_version)?;
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
        starknet_version: StarknetVersion,
    ) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let headers_table = self.txn.open_table(&self.tables.headers)?;
        let starknet_version_table = self.txn.open_table(&self.tables.starknet_version)?;
        let block_hash_to_number_table = self.txn.open_table(&self.tables.block_hash_to_number)?;

        update_marker(&self.txn, &markers_table, block_number)?;

        // Write header.
        headers_table.insert(&self.txn, &block_number, block_header)?;

        // Write mapping.
        update_hash_mapping(&self.txn, &block_hash_to_number_table, block_header, block_number)?;

        // Write StarkNet version if needed.
        update_starknet_version(
            &self.txn,
            &starknet_version_table,
            starknet_version,
            &block_number,
        )?;

        Ok(self)
    }

    fn revert_header(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<BlockHeader>)> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let headers_table = self.txn.open_table(&self.tables.headers)?;
        let block_hash_to_number_table = self.txn.open_table(&self.tables.block_hash_to_number)?;

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
        Ok((self, Some(reverted_header)))
    }
}

fn update_starknet_version(
    txn: &DbTransaction<'_, RW>,
    starknet_version_table: &TableHandle<'_, BlockNumber, StarknetVersion>,
    starknet_version: StarknetVersion,
    block_number: &BlockNumber,
) -> StorageResult<()> {
    let mut cursor = starknet_version_table.cursor(txn)?;
    cursor.lower_bound(block_number)?;
    let res = cursor.prev()?;

    match res {
        Some((_block_number, last_starknet_version))
            if last_starknet_version == starknet_version =>
        {
            Ok(())
        }
        _ => Ok(starknet_version_table.insert(txn, block_number, &starknet_version)?),
    }
}

fn update_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    block_hash_to_number_table: &'env BlockHashToNumberTable<'env>,
    block_header: &BlockHeader,
    block_number: BlockNumber,
) -> Result<(), StorageError> {
    let res = block_hash_to_number_table.insert(txn, &block_header.block_hash, &block_number);
    res.map_err(|err| match err {
        DbError::Inner(libmdbx::Error::KeyExist) => StorageError::BlockHashAlreadyExists {
            block_hash: block_header.block_hash,
            block_number,
        },
        err => err.into(),
    })?;
    Ok(())
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    // Make sure marker is consistent.
    let header_marker = markers_table.get(txn, &MarkerKind::Header)?.unwrap_or_default();
    if header_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: header_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Header, &block_number.next())?;
    Ok(())
}
