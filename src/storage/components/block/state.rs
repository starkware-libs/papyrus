#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use libmdbx::RW;

use super::{BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter};

use crate::starknet::{BlockNumber, StateDiffForward};
use crate::storage::db::{DbTransaction, TableHandle};

// Constants.
const STATE_MARKER_KEY: &[u8] = b"state";

pub trait StateStorageReader {
    fn get_state_marker(&self) -> BlockStorageResult<BlockNumber>;
    fn get_state_diff(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<StateDiffForward>>;
}
pub trait StateStorageWriter {
    fn append_state_diff(
        &mut self,
        block_number: BlockNumber,
        state_diff: &StateDiffForward,
    ) -> BlockStorageResult<()>;
}

impl StateStorageReader for BlockStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_state_marker(&self) -> BlockStorageResult<BlockNumber> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(txn
            .get::<BlockNumber>(&markers_table, STATE_MARKER_KEY)?
            .unwrap_or_default())
    }
    fn get_state_diff(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<StateDiffForward>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;
        let state_diff = txn.get::<StateDiffForward>(
            &state_diffs_table,
            &bincode::serialize(&block_number).unwrap(),
        )?;
        Ok(state_diff)
    }
}

// TODO: multiple writes to the same address / contract may fail the undo invariant.
impl StateStorageWriter for BlockStorageWriter {
    fn append_state_diff(
        &mut self,
        block_number: BlockNumber,
        state_diff: &StateDiffForward,
    ) -> BlockStorageResult<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;

        update_marker(&txn, &markers_table, block_number)?;

        // Write state diff.
        txn.insert(
            &state_diffs_table,
            &bincode::serialize(&block_number).unwrap(),
            &state_diff,
        )?;

        // Finalize.
        txn.commit()?;
        Ok(())
    }
}

fn update_marker(
    txn: &DbTransaction<'_, RW>,
    markers_table: &TableHandle<'_>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    // Make sure marker is consistent.
    let state_marker = txn
        .get::<BlockNumber>(markers_table, STATE_MARKER_KEY)?
        .unwrap_or_default();
    if state_marker != block_number {
        return Err(BlockStorageError::MarkerMismatch {
            expected: state_marker,
            found: block_number,
        });
    };

    // Advance marker.
    txn.upsert(markers_table, STATE_MARKER_KEY, &block_number.next())?;
    Ok(())
}
