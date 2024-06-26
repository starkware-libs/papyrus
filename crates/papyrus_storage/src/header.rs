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
//! #     chain_id: ChainId::Mainnet,
//! #     enforce_file_exists: false,
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

use serde::{Deserialize, Serialize};
use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockSignature,
    BlockTimestamp,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use tracing::debug;

use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::{DbCursorTrait, SimpleTable, Table};
use crate::db::{DbTransaction, TableHandle, TransactionKind, RW};
use crate::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub(crate) struct StorageBlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub state_root: GlobalRoot,
    pub sequencer: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub state_diff_commitment: StateDiffCommitment,
    pub transaction_commitment: Option<TransactionCommitment>,
    pub event_commitment: Option<EventCommitment>,
    pub receipt_commitment: Option<ReceiptCommitment>,
    pub state_diff_length: Option<usize>,
    pub n_transactions: usize,
    pub n_events: usize,
}

type BlockHashToNumberTable<'env> =
    TableHandle<'env, BlockHash, NoVersionValueWrapper<BlockNumber>, SimpleTable>;

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

    /// Returns the signature of the block with the given number.
    fn get_block_signature(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<BlockSignature>>;
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

    /// Removes a block header and its signature (if exists) from the storage and returns the
    /// removed data.
    fn revert_header(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<BlockHeader>, Option<BlockSignature>)>;

    /// Appends a block signature to the storage.
    /// Written separately from the header to allow skipping the signature when creating a block.
    fn append_block_signature(
        self,
        block_number: BlockNumber,
        block_signature: &BlockSignature,
    ) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> HeaderStorageReader for StorageTxn<'env, Mode> {
    fn get_header_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Header)?.unwrap_or_default())
    }

    fn get_block_header(&self, block_number: BlockNumber) -> StorageResult<Option<BlockHeader>> {
        let headers_table = self.open_table(&self.tables.headers)?;
        let Some(block_header) = headers_table.get(&self.txn, &block_number)? else {
            return Ok(None);
        };
        let Some(starknet_version) = self.get_starknet_version(block_number)? else {
            return Ok(None);
        };
        Ok(Some(BlockHeader {
            block_hash: block_header.block_hash,
            parent_hash: block_header.parent_hash,
            block_number: block_header.block_number,
            l1_gas_price: block_header.l1_gas_price,
            l1_data_gas_price: block_header.l1_data_gas_price,
            state_root: block_header.state_root,
            sequencer: block_header.sequencer,
            timestamp: block_header.timestamp,
            l1_da_mode: block_header.l1_da_mode,
            state_diff_commitment: block_header.state_diff_commitment,
            transaction_commitment: block_header.transaction_commitment,
            event_commitment: block_header.event_commitment,
            receipt_commitment: block_header.receipt_commitment,
            state_diff_length: block_header.state_diff_length,
            n_transactions: block_header.n_transactions,
            n_events: block_header.n_events,
            starknet_version,
        }))
    }

    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>> {
        let block_hash_to_number_table = self.open_table(&self.tables.block_hash_to_number)?;
        let block_number = block_hash_to_number_table.get(&self.txn, block_hash)?;
        Ok(block_number)
    }

    // TODO(shahak): Internalize this function.
    fn get_starknet_version(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<StarknetVersion>> {
        if block_number >= self.get_header_marker()? {
            return Ok(None);
        }

        let starknet_version_table = self.open_table(&self.tables.starknet_version)?;
        let mut cursor = starknet_version_table.cursor(&self.txn)?;
        let Some(next_block_number) = block_number.next() else {
            return Ok(None);
        };
        cursor.lower_bound(&next_block_number)?;
        let res = cursor.prev()?;

        match res {
            Some((_block_number, starknet_version)) => Ok(Some(starknet_version)),
            None => unreachable!(
                "Since block_number >= self.get_header_marker(), starknet_version_table should \
                 have at least a single mapping."
            ),
        }
    }

    fn get_block_signature(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<BlockSignature>> {
        let block_signatures_table = self.open_table(&self.tables.block_signatures)?;
        let block_signature = block_signatures_table.get(&self.txn, &block_number)?;
        Ok(block_signature)
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

        let storage_block_header = StorageBlockHeader {
            block_hash: block_header.block_hash,
            parent_hash: block_header.parent_hash,
            block_number: block_header.block_number,
            l1_gas_price: block_header.l1_gas_price,
            l1_data_gas_price: block_header.l1_data_gas_price,
            state_root: block_header.state_root,
            sequencer: block_header.sequencer,
            timestamp: block_header.timestamp,
            l1_da_mode: block_header.l1_da_mode,
            state_diff_commitment: block_header.state_diff_commitment.clone(),
            transaction_commitment: block_header.transaction_commitment,
            event_commitment: block_header.event_commitment,
            receipt_commitment: block_header.receipt_commitment,
            state_diff_length: block_header.state_diff_length,
            n_transactions: block_header.n_transactions,
            n_events: block_header.n_events,
        };

        headers_table.append(&self.txn, &block_number, &storage_block_header)?;

        update_hash_mapping(
            &self.txn,
            &block_hash_to_number_table,
            &storage_block_header,
            block_number,
        )?;

        self.update_starknet_version(&block_number, &block_header.starknet_version)
    }

    // TODO(shahak): Internalize this function.
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
    ) -> StorageResult<(Self, Option<BlockHeader>, Option<BlockSignature>)> {
        let markers_table = self.open_table(&self.tables.markers)?;
        let headers_table = self.open_table(&self.tables.headers)?;
        let block_hash_to_number_table = self.open_table(&self.tables.block_hash_to_number)?;
        let starknet_version_table = self.open_table(&self.tables.starknet_version)?;
        let block_signatures_table = self.open_table(&self.tables.block_signatures)?;

        // Assert that header marker equals the reverted block number + 1
        let current_header_marker = self.get_header_marker()?;

        // Reverts only the last header.
        let Some(next_block_number) = block_number
            .next()
            .filter(|next_block_number| *next_block_number == current_header_marker)
        else {
            debug!(
                "Attempt to revert a non-existing / old header of block {}. Returning without an \
                 action.",
                block_number
            );
            return Ok((self, None, None));
        };

        let reverted_header = headers_table
            .get(&self.txn, &block_number)?
            .expect("Missing header for block {block_number}.");
        markers_table.upsert(&self.txn, &MarkerKind::Header, &block_number)?;
        headers_table.delete(&self.txn, &block_number)?;
        block_hash_to_number_table.delete(&self.txn, &reverted_header.block_hash)?;

        // Revert starknet version and get the version.
        // TODO(shahak): Fix code duplication with get_starknet_version.
        let mut cursor = starknet_version_table.cursor(&self.txn)?;
        cursor.lower_bound(&next_block_number)?;
        let res = cursor.prev()?;

        let starknet_version = match res {
            Some((_block_number, starknet_version)) => starknet_version,
            None => unreachable!(
                "Since block_number >= self.get_header_marker(), starknet_version_table should \
                 have at least a single mapping."
            ),
        };
        starknet_version_table.delete(&self.txn, &block_number)?;

        // Revert block signature.
        let reverted_block_signature = block_signatures_table.get(&self.txn, &block_number)?;
        if reverted_block_signature.is_some() {
            block_signatures_table.delete(&self.txn, &block_number)?;
        }

        Ok((
            self,
            Some(BlockHeader {
                block_hash: reverted_header.block_hash,
                parent_hash: reverted_header.parent_hash,
                block_number: reverted_header.block_number,
                l1_gas_price: reverted_header.l1_gas_price,
                l1_data_gas_price: reverted_header.l1_data_gas_price,
                state_root: reverted_header.state_root,
                sequencer: reverted_header.sequencer,
                timestamp: reverted_header.timestamp,
                l1_da_mode: reverted_header.l1_da_mode,
                state_diff_commitment: reverted_header.state_diff_commitment,
                transaction_commitment: reverted_header.transaction_commitment,
                event_commitment: reverted_header.event_commitment,
                receipt_commitment: reverted_header.receipt_commitment,
                state_diff_length: reverted_header.state_diff_length,
                n_transactions: reverted_header.n_transactions,
                n_events: reverted_header.n_events,
                starknet_version,
            }),
            reverted_block_signature,
        ))
    }

    fn append_block_signature(
        self,
        block_number: BlockNumber,
        block_signature: &BlockSignature,
    ) -> StorageResult<Self> {
        let current_header_marker = self.get_header_marker()?;
        if block_number >= current_header_marker {
            return Err(StorageError::BlockSignatureForNonExistingBlock {
                block_number,
                block_signature: *block_signature,
            });
        }

        let block_signatures_table = self.open_table(&self.tables.block_signatures)?;
        block_signatures_table.insert(&self.txn, &block_number, block_signature)?;
        Ok(self)
    }
}

fn update_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    block_hash_to_number_table: &'env BlockHashToNumberTable<'env>,
    block_header: &StorageBlockHeader,
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
    if header_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: header_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Header, &block_number.unchecked_next())?;
    Ok(())
}
