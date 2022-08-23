#[cfg(test)]
#[path = "body_test.rs"]
mod body_test;

use starknet_api::{
    BlockBody, BlockNumber, Transaction, TransactionHash, TransactionOffsetInBlock,
    TransactionOutput,
};

use super::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use super::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

pub type TransactionsTable<'env> =
    TableHandle<'env, (BlockNumber, TransactionOffsetInBlock), Transaction>;
pub type TransactionOutputsTable<'env> =
    TableHandle<'env, (BlockNumber, TransactionOffsetInBlock), TransactionOutput>;
pub type TransactionHashToIdxTable<'env> =
    TableHandle<'env, TransactionHash, (BlockNumber, TransactionOffsetInBlock)>;

pub trait BodyStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_body_marker(&self) -> StorageResult<BlockNumber>;
    // TODO(spapini): get_transaction_by_hash.
    fn get_transaction(
        &self,
        block_number: BlockNumber,
        tx_offset_in_block: TransactionOffsetInBlock,
    ) -> StorageResult<Option<Transaction>>;
    fn get_transaction_output(
        &self,
        block_number: BlockNumber,
        tx_offset_in_block: TransactionOffsetInBlock,
    ) -> StorageResult<Option<TransactionOutput>>;
    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<(BlockNumber, TransactionOffsetInBlock)>>;
    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Transaction>>>;
    fn get_block_transaction_outputs(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<TransactionOutput>>>;
}
pub trait BodyStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_body(self, block_number: BlockNumber, block_body: &BlockBody) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> BodyStorageReader for StorageTxn<'env, Mode> {
    fn get_body_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Body)?.unwrap_or_default())
    }
    fn get_transaction(
        &self,
        block_number: BlockNumber,
        tx_offset_in_block: TransactionOffsetInBlock,
    ) -> StorageResult<Option<Transaction>> {
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        let transaction = transactions_table.get(&self.txn, &(block_number, tx_offset_in_block))?;
        Ok(transaction)
    }
    fn get_transaction_output(
        &self,
        block_number: BlockNumber,
        tx_offset_in_block: TransactionOffsetInBlock,
    ) -> StorageResult<Option<TransactionOutput>> {
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let transaction_output =
            transaction_outputs_table.get(&self.txn, &(block_number, tx_offset_in_block))?;
        Ok(transaction_output)
    }
    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<(BlockNumber, TransactionOffsetInBlock)>> {
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
        let mut current = cursor.lower_bound(&(block_number, TransactionOffsetInBlock(0)))?;
        let mut res = Vec::new();
        while let Some(((current_block_number, _), tx)) = current {
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
    ) -> StorageResult<Option<Vec<TransactionOutput>>> {
        if self.get_body_marker()? <= block_number {
            return Ok(None);
        }
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let mut cursor = transaction_outputs_table.cursor(&self.txn)?;
        let mut current = cursor.lower_bound(&(block_number, TransactionOffsetInBlock(0)))?;
        let mut res = Vec::new();
        while let Some(((current_block_number, _), tx)) = current {
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
    fn append_body(self, block_number: BlockNumber, block_body: &BlockBody) -> StorageResult<Self> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let transactions_table = self.txn.open_table(&self.tables.transactions)?;
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let transaction_hash_to_idx_table =
            self.txn.open_table(&self.tables.transaction_hash_to_idx)?;

        update_marker(&self.txn, &markers_table, block_number)?;
        write_transactions(
            block_body,
            &self.txn,
            &transactions_table,
            &transaction_hash_to_idx_table,
            block_number,
        )?;
        write_transaction_outputs(block_body, &self.txn, &transaction_outputs_table, block_number)?;

        Ok(self)
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
        transactions_table.insert(txn, &(block_number, tx_offset_in_block), tx)?;
        update_tx_hash_mapping(
            txn,
            transaction_hash_to_idx_table,
            tx,
            block_number,
            tx_offset_in_block,
        )?;
    }
    Ok(())
}

fn write_transaction_outputs<'env>(
    block_body: &BlockBody,
    txn: &DbTransaction<'env, RW>,
    transaction_outputs_table: &'env TransactionOutputsTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for (index, tx_output) in block_body.transaction_outputs.iter().enumerate() {
        let tx_offset_in_block = TransactionOffsetInBlock(index);
        transaction_outputs_table.insert(txn, &(block_number, tx_offset_in_block), tx_output)?;
    }
    Ok(())
}

fn update_tx_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    tx: &Transaction,
    block_number: BlockNumber,
    tx_offset_in_block: TransactionOffsetInBlock,
) -> Result<(), StorageError> {
    let tx_hash = tx.transaction_hash();
    let res = transaction_hash_to_idx_table.insert(
        txn,
        &tx.transaction_hash(),
        &(block_number, tx_offset_in_block),
    );
    res.map_err(|err| match err {
        DbError::InnerDbError(libmdbx::Error::KeyExist) => {
            StorageError::TransactionHashAlreadyExists { tx_hash, block_number, tx_offset_in_block }
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
