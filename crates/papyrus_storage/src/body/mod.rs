#[cfg(test)]
#[path = "body_test.rs"]
mod body_test;
pub mod events;

use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    Event, EventContent, EventIndexInTransactionOutput, Transaction, TransactionHash,
    TransactionOffsetInBlock, TransactionOutput,
};
use tracing::debug;

use crate::body::events::ThinTransactionOutput;
use crate::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use crate::{
    EventIndex, MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn, TransactionIndex,
};

type TransactionsTable<'env> = TableHandle<'env, TransactionIndex, Transaction>;
type TransactionOutputsTable<'env> = TableHandle<'env, TransactionIndex, ThinTransactionOutput>;
type TransactionHashToIdxTable<'env> = TableHandle<'env, TransactionHash, TransactionIndex>;
type EventsTableKey = (ContractAddress, EventIndex);
type EventsTable<'env> = TableHandle<'env, EventsTableKey, EventContent>;

pub trait BodyStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_body_marker(&self) -> StorageResult<BlockNumber>;

    // TODO(spapini): get_transaction_by_hash.
    fn get_transaction(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Transaction>>;

    fn get_transaction_output(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<ThinTransactionOutput>>;

    fn get_transaction_events(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>>;

    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<TransactionIndex>>;

    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Transaction>>>;

    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<ThinTransactionOutput>>>;
}

type RevertedBlockBody = (Vec<Transaction>, Vec<ThinTransactionOutput>, Vec<Vec<EventContent>>);

pub trait BodyStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_body(self, block_number: BlockNumber, block_body: BlockBody) -> StorageResult<Self>;

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
    ) -> StorageResult<Option<Transaction>> {
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

    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Transaction>>> {
        if self.get_body_marker()? <= block_number {
            return Ok(None);
        }
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        let mut cursor = transactions_table.cursor(&self.txn)?;
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

    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<ThinTransactionOutput>>> {
        if self.get_body_marker()? <= block_number {
            return Ok(None);
        }
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let mut cursor = transaction_outputs_table.cursor(&self.txn)?;
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

        update_marker(&self.txn, &markers_table, block_number)?;
        write_transactions(
            &block_body,
            &self.txn,
            &transactions_table,
            &transaction_hash_to_idx_table,
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

        // Delete the transactions data.
        let mut events = vec![];
        for (offset, (tx_output, tx_hash)) in transaction_outputs
            .iter()
            .zip(transactions.iter().map(|t| t.transaction_hash()))
            .enumerate()
        {
            let tx_index = TransactionIndex(block_number, TransactionOffsetInBlock(offset));
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
        }

        markers_table.upsert(&self.txn, &MarkerKind::Body, &block_number)?;
        Ok((self, Some((transactions, transaction_outputs, events))))
    }
}

fn write_transactions<'env>(
    block_body: &BlockBody,
    txn: &DbTransaction<'env, RW>,
    transactions_table: &'env TransactionsTable<'env>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for (index, tx) in block_body.transactions.iter().enumerate() {
        let tx_offset_in_block = TransactionOffsetInBlock(index);
        let transaction_index = TransactionIndex(block_number, tx_offset_in_block);
        transactions_table.insert(txn, &transaction_index, tx)?;
        update_tx_hash_mapping(txn, transaction_hash_to_idx_table, tx, transaction_index)?;
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
    tx: &Transaction,
    transaction_index: TransactionIndex,
) -> Result<(), StorageError> {
    let tx_hash = tx.transaction_hash();
    let res = transaction_hash_to_idx_table.insert(txn, &tx.transaction_hash(), &transaction_index);
    res.map_err(|err| match err {
        DbError::Inner(libmdbx::Error::KeyExist) => {
            StorageError::TransactionHashAlreadyExists { tx_hash, transaction_index }
        }
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
    let body_marker = markers_table.get(txn, &MarkerKind::Body)?.unwrap_or_default();
    if body_marker != block_number {
        return Err(StorageError::MarkerMismatch { expected: body_marker, found: block_number });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Body, &block_number.next())?;
    Ok(())
}
