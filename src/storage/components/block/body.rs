#[cfg(test)]
#[path = "body_test.rs"]
mod body_test;

use libmdbx::RW;

use crate::{
    starknet::{BlockBody, BlockNumber, Transaction, TransactionHash, TransactionIndex},
    storage::db::{DbError, DbTransaction, TableHandle},
};

use super::{
    BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter, MarkerKind,
    MarkersTable,
};

pub type TransactionsTable<'env> = TableHandle<'env, (BlockNumber, TransactionIndex), Transaction>;
pub type TransactionHashToIdxTable<'env> =
    TableHandle<'env, TransactionHash, (BlockNumber, TransactionIndex)>;

pub trait BodyStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_body_marker(&self) -> BlockStorageResult<BlockNumber>;
    // TODO(spapini): get_block_transactions.
    // TODO(spapini): get_transaction_by_hash.
    fn get_transaction(
        &self,
        block_number: BlockNumber,
        tx_index: TransactionIndex,
    ) -> BlockStorageResult<Option<Transaction>>;
    fn get_transaction_idx_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> BlockStorageResult<Option<(BlockNumber, TransactionIndex)>>;
}
pub trait BodyStorageWriter {
    fn append_body(
        &mut self,
        block_number: BlockNumber,
        block_body: &BlockBody,
    ) -> BlockStorageResult<()>;
}
impl BodyStorageReader for BlockStorageReader {
    fn get_body_marker(&self) -> BlockStorageResult<BlockNumber> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(markers_table
            .get(&txn, &MarkerKind::Body)?
            .unwrap_or_default())
    }
    fn get_transaction(
        &self,
        block_number: BlockNumber,
        tx_index: TransactionIndex,
    ) -> BlockStorageResult<Option<Transaction>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let transactions_table = txn.open_table(&self.tables.transactions)?;
        let transaction = transactions_table.get(&txn, &(block_number, tx_index))?;
        Ok(transaction)
    }
    fn get_transaction_idx_by_hash(
        &self,
        block_hash: &TransactionHash,
    ) -> BlockStorageResult<Option<(BlockNumber, TransactionIndex)>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let transaction_hash_to_idx_table = txn.open_table(&self.tables.transaction_hash_to_idx)?;
        let idx = transaction_hash_to_idx_table.get(&txn, block_hash)?;
        Ok(idx)
    }
}
impl BodyStorageWriter for BlockStorageWriter {
    fn append_body(
        &mut self,
        block_number: BlockNumber,
        block_body: &BlockBody,
    ) -> BlockStorageResult<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let transactions_table = txn.open_table(&self.tables.transactions)?;
        let transaction_hash_to_idx_table = txn.open_table(&self.tables.transaction_hash_to_idx)?;

        update_marker(&txn, &markers_table, block_number)?;
        write_transactions(
            block_body,
            &txn,
            &transactions_table,
            &transaction_hash_to_idx_table,
            block_number,
        )?;

        txn.commit()?;
        Ok(())
    }
}

fn write_transactions<'env>(
    block_body: &BlockBody,
    txn: &DbTransaction<'env, RW>,
    transactions_table: &'env TransactionsTable<'env>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    for (index, tx) in block_body.transactions.iter().enumerate() {
        let tx_index = TransactionIndex(index as u64);
        transactions_table.insert(txn, &(block_number, tx_index), tx)?;
        update_tx_hash_mapping(
            txn,
            transaction_hash_to_idx_table,
            tx,
            block_number,
            tx_index,
        )?;
    }
    Ok(())
}

fn update_tx_hash_mapping<'env>(
    txn: &DbTransaction<'env, RW>,
    transaction_hash_to_idx_table: &'env TransactionHashToIdxTable<'env>,
    tx: &Transaction,
    block_number: BlockNumber,
    tx_index: TransactionIndex,
) -> Result<(), BlockStorageError> {
    let tx_hash = tx.transaction_hash();
    let res = transaction_hash_to_idx_table.insert(
        txn,
        &tx.transaction_hash(),
        &(block_number, tx_index),
    );
    res.map_err(|err| match err {
        DbError::InnerDbError(libmdbx::Error::KeyExist) => {
            BlockStorageError::TransactionHashAlreadyExists {
                tx_hash,
                block_number,
                tx_index,
            }
        }
        err => err.into(),
    })?;
    Ok(())
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    // Make sure marker is consistent.
    let body_marker = markers_table
        .get(txn, &MarkerKind::Body)?
        .unwrap_or_default();
    if body_marker != block_number {
        return Err(BlockStorageError::MarkerMismatch {
            expected: body_marker,
            found: block_number,
        });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::Body, &block_number.next())?;
    Ok(())
}
