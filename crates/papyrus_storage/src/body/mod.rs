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
//! # use papyrus_storage::db::DbConfig;
//! # use starknet_api::core::ChainId;
//! use starknet_api::block::{Block, BlockNumber};
//! use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
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
//! let (reader, mut writer) = open_storage(db_config)?;
//! writer
//!     .begin_rw_txn()?                                // Start a RW transaction.
//!     .append_body(BlockNumber(0), block.body.clone())?       // Append the block body (consumes the body at the current version).
//!     .commit()?;
//!
//! let stored_body_transactions = reader.begin_ro_txn()?.get_block_transactions(BlockNumber(0))?;
//! let expected_transactions_with_exec_statuses = block.body.transactions.iter().cloned().zip(block.body.transaction_execution_statuses.iter().cloned()).collect::<Vec<_>>();
//! assert_eq!(stored_body_transactions, Some(expected_transactions_with_exec_statuses));
//! # Ok::<(), papyrus_storage::StorageError>(())
//! ```

#[cfg(test)]
#[path = "body_test.rs"]
mod body_test;
pub mod events;

use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    Event, EventContent, EventIndexInTransactionOutput, Transaction, TransactionExecutionStatus,
    TransactionHash, TransactionOffsetInBlock, TransactionOutput,
};
use tracing::debug;

use crate::body::events::{EventIndex, ThinTransactionOutput};
use crate::db::serialization::StorageSerde;
use crate::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use crate::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

type TransactionsTable<'env> =
    TableHandle<'env, TransactionIndex, (Transaction, TransactionExecutionStatus)>;
type TransactionOutputsTable<'env> = TableHandle<'env, TransactionIndex, ThinTransactionOutput>;
type TransactionHashToIdxTable<'env> = TableHandle<'env, TransactionHash, TransactionIndex>;
type TransactionIdxToHashTable<'env> = TableHandle<'env, TransactionIndex, TransactionHash>;
type EventsTableKey = (ContractAddress, EventIndex);
type EventsTable<'env> = TableHandle<'env, EventsTableKey, EventContent>;

/// The index of a transaction in a block.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TransactionIndex(pub BlockNumber, pub TransactionOffsetInBlock);

/// Interface for reading data related to the block body.
pub trait BodyStorageReader {
    /// The body marker is the first block number that doesn't exist yet.
    fn get_body_marker(&self) -> StorageResult<BlockNumber>;

    /// Returns the transaction and its execution status at the given index.
    fn get_transaction(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<(Transaction, TransactionExecutionStatus)>>;

    /// Returns the transaction output at the given index.
    fn get_transaction_output(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<ThinTransactionOutput>>;

    /// Returns the events of the transaction output at the given index.
    fn get_transaction_events(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>>;

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
    ) -> StorageResult<Option<Vec<(Transaction, TransactionExecutionStatus)>>>;

    /// Returns the transaction hashes of the block with the given number.
    fn get_block_transaction_hashes(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionHash>>>;

    /// Returns the transaction outputs of the block with the given number.
    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<ThinTransactionOutput>>>;
}

type RevertedBlockBody = (
    Vec<(Transaction, TransactionExecutionStatus)>,
    Vec<ThinTransactionOutput>,
    Vec<TransactionHash>,
    Vec<Vec<EventContent>>,
);

/// Interface for updating data related to the block body.
pub trait BodyStorageWriter
where
    Self: Sized,
{
    /// Appends a block body to the storage.
    /// # Panics
    /// This function will panic if block_body contains transaction hashes and receipts of different
    /// lengths.
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
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Body)?.unwrap_or_default())
    }

    fn get_transaction(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<(Transaction, TransactionExecutionStatus)>> {
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        let transaction = transactions_table.get(&self.txn, &transaction_index)?;
        Ok(transaction)
    }

    fn get_transaction_output(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<ThinTransactionOutput>> {
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let transaction_output = transaction_outputs_table.get(&self.txn, &transaction_index)?;
        Ok(transaction_output)
    }

    fn get_transaction_events(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>> {
        let tx_output = self.get_transaction_output(transaction_index)?;
        if tx_output.is_none() {
            return Ok(None);
        }
        let events_table = self.txn.open_table(&self.tables.events)?;

        let mut res = Vec::new();
        for (index, from_address) in
            tx_output.unwrap().events_contract_addresses().into_iter().enumerate()
        {
            let event_index = EventIndex(transaction_index, EventIndexInTransactionOutput(index));
            if let Some(content) = events_table.get(&self.txn, &(from_address, event_index))? {
                res.push(Event { from_address, content });
            } else {
                return Err(StorageError::EventNotFound { event_index, from_address });
            }
        }

        Ok(Some(res))
    }

    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<TransactionIndex>> {
        let transaction_hash_to_idx_table =
            self.txn.open_table(&self.tables.transaction_hash_to_idx)?;
        let idx = transaction_hash_to_idx_table.get(&self.txn, tx_hash)?;
        Ok(idx)
    }

    fn get_transaction_hash_by_idx(
        &self,
        tx_index: &TransactionIndex,
    ) -> StorageResult<Option<TransactionHash>> {
        let transaction_idx_to_hash_table =
            self.txn.open_table(&self.tables.transaction_idx_to_hash)?;
        let idx = transaction_idx_to_hash_table.get(&self.txn, tx_index)?;
        Ok(idx)
    }

    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<(Transaction, TransactionExecutionStatus)>>> {
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        self.get_transactions_in_block(block_number, transactions_table)
    }

    fn get_block_transaction_hashes(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionHash>>> {
        let transaction_idx_to_hash_table =
            self.txn.open_table(&self.tables.transaction_idx_to_hash)?;
        self.get_transactions_in_block(block_number, transaction_idx_to_hash_table)
    }

    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<ThinTransactionOutput>>> {
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        self.get_transactions_in_block(block_number, transaction_outputs_table)
    }
}

impl<'env, Mode: TransactionKind> StorageTxn<'env, Mode> {
    // Helper function to get from 'table' all the values of entries with transaction index in
    // 'block_number'. The returned values are ordered by the transaction offset in block in
    // ascending order.
    fn get_transactions_in_block<V: StorageSerde>(
        &self,
        block_number: BlockNumber,
        table: TableHandle<'env, TransactionIndex, V>,
    ) -> StorageResult<Option<Vec<V>>> {
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
}

impl<'env> BodyStorageWriter for StorageTxn<'env, RW> {
    fn append_body(self, block_number: BlockNumber, block_body: BlockBody) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let events_table = self.txn.open_table(&self.tables.events)?;
        let transaction_hash_to_idx_table =
            self.txn.open_table(&self.tables.transaction_hash_to_idx)?;
        let transaction_idx_to_hash_table =
            self.txn.open_table(&self.tables.transaction_idx_to_hash)?;

        update_marker(&self.txn, &markers_table, block_number)?;
        write_transactions(
            &block_body,
            &self.txn,
            &transactions_table,
            &transaction_hash_to_idx_table,
            &transaction_idx_to_hash_table,
            block_number,
        )?;
        write_transaction_outputs(
            block_body,
            &self.txn,
            &transaction_outputs_table,
            &events_table,
            block_number,
        )?;

        Ok(self)
    }

    fn revert_body(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<RevertedBlockBody>)> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let transaction_hash_to_idx_table =
            self.txn.open_table(&self.tables.transaction_hash_to_idx)?;
        let transaction_idx_to_hash_table =
            self.txn.open_table(&self.tables.transaction_idx_to_hash)?;
        let events_table = self.txn.open_table(&self.tables.events)?;

        // Assert that body marker equals the reverted block number + 1
        let current_header_marker = self.get_body_marker()?;
        if current_header_marker != block_number.next() {
            debug!(
                "Attempt to revert a non-existing / old block {}. Returning without an action.",
                block_number
            );
            return Ok((self, None));
        }

        let transactions = self
            .get_block_transactions(block_number)?
            .expect("Missing transactions for block {block_number}.");
        let transaction_outputs = self
            .get_block_transaction_outputs(block_number)?
            .expect("Missing transaction outputs for block {block_number}.");
        let transaction_hashes = self
            .get_block_transaction_hashes(block_number)?
            .expect("Missing transaction hashes for block {block_number}.");

        // Delete the transactions data.
        let mut events = vec![];
        for (offset, tx_output) in transaction_outputs.iter().enumerate() {
            let tx_index = TransactionIndex(block_number, TransactionOffsetInBlock(offset));
            let tx_hash = self
                .get_transaction_hash_by_idx(&tx_index)?
                .expect("Missing transaction hash for transaction index {tx_index}.");
            let mut tx_events = vec![];
            for (index, from_address) in
                tx_output.events_contract_addresses_as_ref().iter().enumerate()
            {
                let key =
                    (*from_address, EventIndex(tx_index, EventIndexInTransactionOutput(index)));
                tx_events.push(
                    events_table
                        .get(&self.txn, &key)?
                        .expect("Missing events for transaction output {tx_index}."),
                );
                events_table.delete(&self.txn, &key)?;
            }
            events.push(tx_events);
            transactions_table.delete(&self.txn, &tx_index)?;
            transaction_outputs_table.delete(&self.txn, &tx_index)?;
            transaction_hash_to_idx_table.delete(&self.txn, &tx_hash)?;
            transaction_idx_to_hash_table.delete(&self.txn, &tx_index)?;
        }

        markers_table.upsert(&self.txn, &MarkerKind::Body, &block_number)?;
        Ok((self, Some((transactions, transaction_outputs, transaction_hashes, events))))
    }
}

// This function will panic if block_body contains transaction hashes and receipts of different
// lengths.
fn write_transactions<'env>(
    block_body: &BlockBody,
    txn: &DbTransaction<'env, RW>,
    transactions_table: &'env TransactionsTable<'env>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    transaction_idx_to_hash_table: &'env TransactionIdxToHashTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for index in 0..block_body.transactions.len() {
        let tx_offset_in_block = TransactionOffsetInBlock(index);
        let transaction_index = TransactionIndex(block_number, tx_offset_in_block);
        let transaction_hash = block_body.transaction_hashes[index];
        // A transaction must have an execution status.
        let tx = block_body.transactions[index].clone();
        let exec_status = block_body.transaction_execution_statuses[index].clone();
        update_tx_hash_mapping(
            txn,
            transaction_hash_to_idx_table,
            transaction_idx_to_hash_table,
            transaction_hash,
            transaction_index,
        )?;
        transactions_table.insert(txn, &transaction_index, &(tx, exec_status))?;
    }
    Ok(())
}

fn write_transaction_outputs<'env>(
    block_body: BlockBody,
    txn: &DbTransaction<'env, RW>,
    transaction_outputs_table: &'env TransactionOutputsTable<'env>,
    events_table: &'env EventsTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for (index, tx_output) in block_body.transaction_outputs.into_iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(index));

        write_events(&tx_output, txn, events_table, transaction_index)?;
        transaction_outputs_table.insert(
            txn,
            &transaction_index,
            &ThinTransactionOutput::from(tx_output),
        )?;
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
        events_table.insert(txn, &(event.from_address, event_index), &event.content)?;
    }
    Ok(())
}

fn update_tx_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    transaction_idx_to_hash_table: &'env TransactionIdxToHashTable<'env>,
    tx_hash: TransactionHash,
    transaction_index: TransactionIndex,
) -> Result<(), StorageError> {
    let res = transaction_hash_to_idx_table.insert(txn, &tx_hash, &transaction_index);
    res.map_err(|err| match err {
        DbError::Inner(libmdbx::Error::KeyExist) => {
            StorageError::TransactionHashAlreadyExists { tx_hash, transaction_index }
        }
        err => err.into(),
    })?;
    transaction_idx_to_hash_table.insert(txn, &transaction_index, &tx_hash)?;
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

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Body, &block_number.next())?;
    Ok(())
}
