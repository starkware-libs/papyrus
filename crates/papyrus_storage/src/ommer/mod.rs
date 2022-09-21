use starknet_api::{
    BlockBody, BlockHash, BlockHeader, Transaction, TransactionOffsetInBlock, TransactionOutput,
};

use crate::db::{DbTransaction, TableHandle, RW};
use crate::{StorageResult, StorageTxn, ThinStateDiff};

#[cfg(test)]
#[path = "ommer_test.rs"]
mod ommer_test;

pub trait OmmerStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn insert_ommer_block(
        self,
        block_hash: BlockHash,
        block_header: &BlockHeader,
        block_body: &BlockBody,
        state_diff: &ThinStateDiff,
    ) -> StorageResult<Self>;
}

impl<'env> OmmerStorageWriter for StorageTxn<'env, RW> {
    fn insert_ommer_block(
        self,
        block_hash: BlockHash,
        block_header: &BlockHeader,
        block_body: &BlockBody,
        _state_diff: &ThinStateDiff,
    ) -> StorageResult<Self> {
        let headers_table = self.txn.open_table(&self.tables.ommer_headers)?;
        let transaction_outputs_table =
            self.txn.open_table(&self.tables.ommer_transaction_outputs)?;
        let transactions_table = self.txn.open_table(&self.tables.ommer_transactions)?;

        insert_ommer_header(&self.txn, &headers_table, block_hash, block_header)?;
        insert_ommer_body(
            &self.txn,
            &transaction_outputs_table,
            &transactions_table,
            block_hash,
            block_body,
        )?;

        // TODO(yair): insert state_diff
        Ok(self)
    }
}

type HeadersTable<'env> = TableHandle<'env, BlockHash, BlockHeader>;
fn insert_ommer_header<'env>(
    txn: &DbTransaction<'env, RW>,
    headers_table: &'env HeadersTable<'env>,
    block_hash: BlockHash,
    block_header: &BlockHeader,
) -> StorageResult<()> {
    headers_table.insert(txn, &block_hash, block_header)?;
    Ok(())
}

type TransactionOutputsTable<'env> =
    TableHandle<'env, (BlockHash, TransactionOffsetInBlock), TransactionOutput>;
type TransactionsTable<'env> =
    TableHandle<'env, (BlockHash, TransactionOffsetInBlock), Transaction>;
fn insert_ommer_body<'env>(
    txn: &DbTransaction<'env, RW>,
    transaction_outputs_table: &'env TransactionOutputsTable<'env>,
    transactions_table: &'env TransactionsTable<'env>,
    block_hash: BlockHash,
    block_body: &BlockBody,
) -> StorageResult<()> {
    let transactions_iter = block_body.transactions().iter();
    let transaction_outputs_iter = block_body.transaction_outputs().iter();

    for (index, (tx, tx_output)) in transactions_iter.zip(transaction_outputs_iter).enumerate() {
        let tx_offset_in_block = TransactionOffsetInBlock(index);
        let key = (block_hash, tx_offset_in_block);
        transactions_table.insert(txn, &key, tx)?;
        transaction_outputs_table.insert(txn, &key, tx_output)?;
    }
    Ok(())
}
