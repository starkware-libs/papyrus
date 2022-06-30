#[cfg(test)]
#[path = "body_test.rs"]
mod body_test;

use libmdbx::RW;

use crate::{
    starknet::{BlockBody, BlockNumber, Transaction, TransactionIndex},
    storage::db::{DbTransaction, TableHandle},
};

use super::{BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter};

// Constants.
const BODY_MARKER_KEY: &[u8] = b"body";

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
        Ok(txn
            .get::<BlockNumber>(&markers_table, BODY_MARKER_KEY)?
            .unwrap_or_default())
    }
    fn get_transaction(
        &self,
        block_number: BlockNumber,
        tx_index: TransactionIndex,
    ) -> BlockStorageResult<Option<Transaction>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let transactions_table = txn.open_table(&self.tables.transactions)?;
        let transaction = txn.get::<Transaction>(
            &transactions_table,
            &bincode::serialize(&(block_number, tx_index)).unwrap(),
        )?;
        Ok(transaction)
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

        update_marker(&txn, &markers_table, block_number)?;
        write_transactions(block_body, &txn, &transactions_table, block_number)?;

        txn.commit()?;
        Ok(())
    }
}

fn write_transactions(
    block_body: &BlockBody,
    txn: &DbTransaction<'_, RW>,
    transactions_table: &TableHandle<'_>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    for (index, tx) in block_body.transactions.iter().enumerate() {
        txn.insert(
            transactions_table,
            &bincode::serialize(&(block_number, TransactionIndex(index as u64))).unwrap(),
            tx,
        )?;
    }
    Ok(())
}

fn update_marker(
    txn: &DbTransaction<'_, RW>,
    markers_table: &TableHandle<'_>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    // Make sure marker is consistent.
    let body_marker = txn
        .get::<BlockNumber>(markers_table, BODY_MARKER_KEY)?
        .unwrap_or_default();
    if body_marker != block_number {
        return Err(BlockStorageError::MarkerMismatch {
            expected: body_marker,
            found: block_number,
        });
    };

    // Advance marker.
    txn.upsert(markers_table, BODY_MARKER_KEY, &block_number.next())?;
    Ok(())
}
