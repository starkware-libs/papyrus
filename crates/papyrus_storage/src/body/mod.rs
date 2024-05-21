//! Interface for handling data related to Starknet [block bodies](https://docs.rs/starknet_api/latest/starknet_api/block/struct.BlockBody.html).
//!
//! The block body is the part of the block that contains the transactions and the transaction
//! outputs.
//! Import [`BodyStorageReader`] and [`BodyStorageWriter`] to read and write data related
//! to the block bodies using a [`StorageTxn`].
//!
//! See [`events`] module for the interface for handling events.
//!
//!  # Example
//! ```
//! use papyrus_storage::open_storage;
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! use starknet_api::block::{Block, BlockNumber};
//! use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId("SN_MAIN".to_owned()),
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
//!     .append_body(BlockNumber(0), block.body)?       // Append the block body (consumes the body at the current version).
//!     .commit()?;
//!
//! let stored_body_transactions = reader.begin_ro_txn()?.get_block_transactions(BlockNumber(0))?;
//! assert_eq!(stored_body_transactions, Some(Block::default().body.transactions));
//! # Ok::<(), papyrus_storage::StorageError>(())
//! ```

#[cfg(test)]
mod body_test;
pub mod events;

use std::fmt::Debug;

use papyrus_proc_macros::latency_histogram;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    EventIndexInTransactionOutput,
    Transaction,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionOutput,
};
use tracing::debug;

use crate::body::events::EventIndex;
use crate::db::serialization::{NoVersionValueWrapper, ValueSerde, VersionZeroWrapper};
use crate::db::table_types::{DbCursorTrait, NoValue, SimpleTable, Table};
use crate::db::{DbTransaction, TableHandle, TransactionKind, RW};
use crate::mmap_file::LocationInFile;
use crate::{
    FileHandlers,
    MarkerKind,
    MarkersTable,
    StorageError,
    StorageResult,
    StorageScope,
    StorageTxn,
};

type TransactionsTable<'env> =
    TableHandle<'env, TransactionIndex, VersionZeroWrapper<LocationInFile>, SimpleTable>;
type TransactionOutputsTable<'env> =
    TableHandle<'env, TransactionIndex, VersionZeroWrapper<LocationInFile>, SimpleTable>;
type TransactionHashToIdxTable<'env> =
    TableHandle<'env, TransactionHash, NoVersionValueWrapper<TransactionIndex>, SimpleTable>;
type TransactionIdxToHashTable<'env> =
    TableHandle<'env, TransactionIndex, NoVersionValueWrapper<TransactionHash>, SimpleTable>;
type EventsTableKey = (ContractAddress, EventIndex);
type EventsTable<'env> =
    TableHandle<'env, EventsTableKey, NoVersionValueWrapper<NoValue>, SimpleTable>;

/// The index of a transaction in a block.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize, PartialOrd, Ord)]
#[cfg_attr(any(test, feature = "testing"), derive(Hash))]
pub struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);

/// Interface for reading data related to the block body.
pub trait BodyStorageReader {
    /// The body marker is the first block number that doesn't exist yet.
    fn get_body_marker(&self) -> StorageResult<BlockNumber>;

    /// Returns the transaction and its execution status at the given index.
    fn get_transaction(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Transaction>>;

    /// Returns the transaction output at the given index.
    fn get_transaction_output(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<TransactionOutput>>;

    /// Returns the index of the transaction with the given hash.
    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<TransactionIndex>>;

    /// Returns the transaction hash with the given transaction index.
    fn get_transaction_hash_by_idx(
        &self,
        tx_index: &TransactionIndex,
    ) -> StorageResult<Option<TransactionHash>>;

    /// Returns the transactions and their execution status of the block with the given number.
    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Transaction>>>;

    /// Returns the transaction hashes of the block with the given number.
    fn get_block_transaction_hashes(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionHash>>>;

    /// Returns the transaction outputs of the block with the given number.
    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionOutput>>>;

    /// Returns the number of transactions in the block with the given number.
    fn get_block_transactions_count(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<usize>>;
}

type RevertedBlockBody = (Vec<Transaction>, Vec<TransactionOutput>, Vec<TransactionHash>);

/// Interface for updating data related to the block body.
pub trait BodyStorageWriter
where
    Self: Sized,
{
    /// Appends a block body to the storage.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    // The body is consumed to avoid unnecessary copying while converting transaction outputs into
    // thin transaction outputs.
    // TODO(yair): make this work without consuming the body.
    fn append_body(self, block_number: BlockNumber, block_body: BlockBody) -> StorageResult<Self>;

    /// Removes a block body from the storage and returns the removed data.
    fn revert_body(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedBlockBody>)>;
}

impl<'env, Mode: TransactionKind> BodyStorageReader for StorageTxn<'env, Mode> {
    fn get_body_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Body)?.unwrap_or_default())
    }

    fn get_transaction(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Transaction>> {
        let transactions_table = self.open_table(&self.tables.transactions)?;
        let Some(tx_location) = transactions_table.get(&self.txn, &transaction_index)? else {
            return Ok(None);
        };
        let transaction = self.file_handlers.get_transaction_unchecked(tx_location)?;
        Ok(Some(transaction))
    }

    fn get_transaction_output(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<TransactionOutput>> {
        let transaction_outputs_table = self.open_table(&self.tables.transaction_outputs)?;
        let Some(tx_output_location) =
            transaction_outputs_table.get(&self.txn, &transaction_index)?
        else {
            return Ok(None);
        };
        let transaction_output =
            self.file_handlers.get_transaction_output_unchecked(tx_output_location)?;
        Ok(Some(transaction_output))
    }

    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<TransactionIndex>> {
        let transaction_hash_to_idx_table =
            self.open_table(&self.tables.transaction_hash_to_idx)?;
        let idx = transaction_hash_to_idx_table.get(&self.txn, tx_hash)?;
        Ok(idx)
    }

    fn get_transaction_hash_by_idx(
        &self,
        tx_index: &TransactionIndex,
    ) -> StorageResult<Option<TransactionHash>> {
        let transaction_idx_to_hash_table =
            self.open_table(&self.tables.transaction_idx_to_hash)?;
        let idx = transaction_idx_to_hash_table.get(&self.txn, tx_index)?;
        Ok(idx)
    }

    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Transaction>>> {
        let transactions_table = self.open_table(&self.tables.transactions)?;
        self.get_transaction_objects_in_block(block_number, transactions_table)
    }

    fn get_block_transaction_hashes(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionHash>>> {
        let transaction_idx_to_hash_table =
            self.open_table(&self.tables.transaction_idx_to_hash)?;
        self.get_transactions_in_block(block_number, transaction_idx_to_hash_table)
    }

    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionOutput>>> {
        let transaction_outputs_table = self.open_table(&self.tables.transaction_outputs)?;
        self.get_transaction_outputs_in_block(block_number, transaction_outputs_table)
    }

    fn get_block_transactions_count(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<usize>> {
        // After this condition, we know that the block exists, so if something goes wrong is only
        // because there are no transactions in it.
        if self.get_body_marker()? <= block_number {
            return Ok(None);
        }

        let transactions_table = self.open_table(&self.tables.transaction_idx_to_hash)?;
        let mut cursor = transactions_table.cursor(&self.txn)?;
        let Some(next_block_number) = block_number.next() else {
            return Ok(None);
        };

        cursor.lower_bound(&TransactionIndex(next_block_number, TransactionOffsetInBlock(0)))?;
        let Some((TransactionIndex(received_block_number, last_tx_index), _tx_hash)) =
            cursor.prev()?
        else {
            return Ok(Some(0));
        };
        if received_block_number != block_number {
            return Ok(Some(0));
        }

        Ok(Some(last_tx_index.0 + 1))
    }
}

impl<'env, Mode: TransactionKind> StorageTxn<'env, Mode> {
    // Helper function to get from 'table' all the values of entries with transaction index in
    // 'block_number'. The returned values are ordered by the transaction offset in block in
    // ascending order.
    fn get_transactions_in_block<V: ValueSerde + Debug>(
        &self,
        block_number: BlockNumber,
        table: TableHandle<'env, TransactionIndex, V, SimpleTable>,
    ) -> StorageResult<Option<Vec<V::Value>>> {
        if self.get_body_marker()? <= block_number {
            return Ok(None);
        }
        let mut cursor = table.cursor(&self.txn)?;
        let mut current =
            cursor.lower_bound(&TransactionIndex(block_number, TransactionOffsetInBlock(0)))?;
        let mut res = Vec::new();
        while let Some((TransactionIndex(current_block_number, _), tx)) = current {
            if current_block_number != block_number {
                break;
            }
            res.push(tx);
            current = cursor.next()?;
        }
        Ok(Some(res))
    }

    // TODO(dvir): remove this function when we have a general table interface also for values
    // written to a file.
    // Returns the transaction outputs in the given block.
    fn get_transaction_outputs_in_block(
        &self,
        block_number: BlockNumber,
        transaction_output_offsets_table: TransactionOutputsTable<'env>,
    ) -> StorageResult<Option<Vec<TransactionOutput>>> {
        let Some(locations) =
            self.get_transactions_in_block(block_number, transaction_output_offsets_table)?
        else {
            return Ok(None);
        };

        let res = locations
            .into_iter()
            .map(|location| self.file_handlers.get_transaction_output_unchecked(location))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(res))
    }

    // TODO(dvir): remove this function when we have a general table interface also for values
    // written to a file.
    // Returns the transactions in the given block.
    fn get_transaction_objects_in_block(
        &self,
        block_number: BlockNumber,
        transaction_offsets_table: TransactionsTable<'env>,
    ) -> StorageResult<Option<Vec<Transaction>>> {
        let Some(locations) =
            self.get_transactions_in_block(block_number, transaction_offsets_table)?
        else {
            return Ok(None);
        };

        let res = locations
            .into_iter()
            .map(|location| self.file_handlers.get_transaction_unchecked(location))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(res))
    }
}

impl<'env> BodyStorageWriter for StorageTxn<'env, RW> {
    #[latency_histogram("storage_append_body_latency_seconds", false)]
    fn append_body(self, block_number: BlockNumber, block_body: BlockBody) -> StorageResult<Self> {
        let markers_table = self.open_table(&self.tables.markers)?;
        update_marker(&self.txn, &markers_table, block_number)?;

        if self.scope != StorageScope::StateOnly {
            let transactions_table = self.open_table(&self.tables.transactions)?;
            let transaction_outputs_table = self.open_table(&self.tables.transaction_outputs)?;
            let events_table = self.open_table(&self.tables.events)?;
            let transaction_hash_to_idx_table =
                self.open_table(&self.tables.transaction_hash_to_idx)?;
            let transaction_idx_to_hash_table =
                self.open_table(&self.tables.transaction_idx_to_hash)?;

            write_transactions(
                &block_body,
                &self.txn,
                &self.file_handlers,
                &transactions_table,
                &transaction_hash_to_idx_table,
                &transaction_idx_to_hash_table,
                block_number,
            )?;
            write_transaction_outputs(
                block_body,
                &self.txn,
                &self.file_handlers,
                &transaction_outputs_table,
                &events_table,
                block_number,
            )?;
        }

        Ok(self)
    }

    fn revert_body(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedBlockBody>)> {
        let markers_table = self.open_table(&self.tables.markers)?;

        // Assert that body marker equals the reverted block number + 1
        let current_header_marker = self.get_body_marker()?;
        if block_number
            .next()
            .filter(|next_block_number| current_header_marker == *next_block_number)
            .is_none()
        {
            debug!(
                "Attempt to revert a non-existing / old block {}. Returning without an action.",
                block_number
            );
            return Ok((self, None));
        }

        let reverted_block_body = 'reverted_block_body: {
            if self.scope == StorageScope::StateOnly {
                break 'reverted_block_body None;
            }

            let transactions_table = self.open_table(&self.tables.transactions)?;
            let transaction_outputs_table = self.open_table(&self.tables.transaction_outputs)?;
            let transaction_hash_to_idx_table =
                self.open_table(&self.tables.transaction_hash_to_idx)?;
            let transaction_idx_to_hash_table =
                self.open_table(&self.tables.transaction_idx_to_hash)?;
            let events_table = self.open_table(&self.tables.events)?;

            let transactions = self
                .get_block_transactions(block_number)?
                .unwrap_or_else(|| panic!("Missing transactions for block {block_number}."));
            let transaction_outputs = self
                .get_block_transaction_outputs(block_number)?
                .unwrap_or_else(|| panic!("Missing transaction outputs for block {block_number}."));
            let transaction_hashes = self
                .get_block_transaction_hashes(block_number)?
                .unwrap_or_else(|| panic!("Missing transaction hashes for block {block_number}."));

            // Delete the transactions data.
            for (offset, (tx_hash, tx_output)) in
                transaction_hashes.iter().zip(transaction_outputs.iter()).enumerate()
            {
                let tx_index = TransactionIndex(block_number, TransactionOffsetInBlock(offset));

                for (event_offset, event) in tx_output.events().iter().enumerate() {
                    let key = (
                        event.from_address,
                        EventIndex(tx_index, EventIndexInTransactionOutput(event_offset)),
                    );
                    events_table.delete(&self.txn, &key)?;
                }
                transactions_table.delete(&self.txn, &tx_index)?;
                transaction_outputs_table.delete(&self.txn, &tx_index)?;
                transaction_hash_to_idx_table.delete(&self.txn, tx_hash)?;
                transaction_idx_to_hash_table.delete(&self.txn, &tx_index)?;
            }
            Some((transactions, transaction_outputs, transaction_hashes))
        };

        markers_table.upsert(&self.txn, &MarkerKind::Body, &block_number)?;
        markers_table.upsert(&self.txn, &MarkerKind::Event, &block_number)?;
        Ok((self, reverted_block_body))
    }
}

fn write_transactions<'env>(
    block_body: &BlockBody,
    txn: &DbTransaction<'env, RW>,
    file_handlers: &FileHandlers<RW>,
    transactions_table: &'env TransactionsTable<'env>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    transaction_idx_to_hash_table: &'env TransactionIdxToHashTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for (index, (tx, tx_hash)) in
        block_body.transactions.iter().zip(block_body.transaction_hashes.iter()).enumerate()
    {
        let tx_offset_in_block = TransactionOffsetInBlock(index);
        let transaction_index = TransactionIndex(block_number, tx_offset_in_block);
        update_tx_hash_mapping(
            txn,
            transaction_hash_to_idx_table,
            transaction_idx_to_hash_table,
            tx_hash,
            transaction_index,
        )?;
        let location = file_handlers.append_transaction(tx);
        transactions_table.insert(txn, &transaction_index, &location)?;
    }
    Ok(())
}

fn write_transaction_outputs<'env>(
    block_body: BlockBody,
    txn: &DbTransaction<'env, RW>,
    file_handlers: &FileHandlers<RW>,
    transaction_outputs_table: &'env TransactionOutputsTable<'env>,
    events_table: &'env EventsTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for (index, tx_output) in block_body.transaction_outputs.into_iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(index));

        write_events(&tx_output, txn, events_table, transaction_index)?;
        let location = file_handlers.append_transaction_output(&tx_output);
        transaction_outputs_table.insert(txn, &transaction_index, &location)?;
    }
    Ok(())
}

fn write_events<'env>(
    tx_output: &TransactionOutput,
    txn: &DbTransaction<'env, RW>,
    events_table: &'env EventsTable<'env>,
    transaction_index: TransactionIndex,
) -> StorageResult<()> {
    for (index, event) in tx_output.events().iter().enumerate() {
        let event_index = EventIndex(transaction_index, EventIndexInTransactionOutput(index));
        events_table.insert(txn, &(event.from_address, event_index), &NoValue)?;
    }
    Ok(())
}

fn update_tx_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    transaction_idx_to_hash_table: &'env TransactionIdxToHashTable<'env>,
    tx_hash: &TransactionHash,
    transaction_index: TransactionIndex,
) -> Result<(), StorageError> {
    transaction_hash_to_idx_table.insert(txn, tx_hash, &transaction_index)?;
    transaction_idx_to_hash_table.insert(txn, &transaction_index, tx_hash)?;
    Ok(())
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    // Make sure marker is consistent.
    let body_marker = markers_table.get(txn, &MarkerKind::Body)?.unwrap_or_default();
    if body_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: body_marker, found: block_number });
    };
    let event_marker = markers_table.get(txn, &MarkerKind::Event)?.unwrap_or_default();
    if event_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: event_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Body, &block_number.unchecked_next())?;
    markers_table.upsert(txn, &MarkerKind::Event, &block_number.unchecked_next())?;
    Ok(())
}
