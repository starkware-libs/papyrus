#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use libmdbx::RW;

use super::{BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter};

use crate::starknet::{
    BlockNumber, IndexedDeployedContract, StateDiffForward, StorageDiff, StorageEntry,
};
use crate::storage::db::{DbError, DbTransaction, TableHandle};

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

impl StateStorageWriter for BlockStorageWriter {
    fn append_state_diff(
        &mut self,
        block_number: BlockNumber,
        state_diff: &StateDiffForward,
    ) -> BlockStorageResult<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let contracts_table = txn.open_table(&self.tables.contracts)?;
        let storage_table = txn.open_table(&self.tables.contract_storage)?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;

        update_marker(&txn, &markers_table, block_number)?;
        // Write state diff.
        txn.insert(
            &state_diffs_table,
            &bincode::serialize(&block_number).unwrap(),
            &state_diff,
        )?;
        // Write state.
        write_deployed_contracts(state_diff, &txn, block_number, &contracts_table)?;
        write_storage_diffs(state_diff, &txn, block_number, &storage_table)?;

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

fn write_deployed_contracts(
    state_diff: &StateDiffForward,
    txn: &DbTransaction<'_, RW>,
    block_number: BlockNumber,
    contracts_table: &TableHandle<'_>,
) -> BlockStorageResult<()> {
    for deployed_contract in &state_diff.deployed_contracts {
        let key = bincode::serialize(&deployed_contract.address).unwrap();
        let class_hash = deployed_contract.class_hash;
        let value = IndexedDeployedContract {
            block_number,
            class_hash,
        };
        let res = txn.insert(contracts_table, &key, &value);
        match res {
            Ok(()) => continue,
            Err(DbError::InnerDbError(libmdbx::Error::KeyExist)) => {
                return Err(BlockStorageError::ContractAlreadyExists {
                    address: deployed_contract.address,
                });
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}
fn write_storage_diffs(
    state_diff: &StateDiffForward,
    txn: &DbTransaction<'_, RW>,
    block_number: BlockNumber,
    storage_table: &TableHandle<'_>,
) -> BlockStorageResult<()> {
    for StorageDiff { address, diff } in &state_diff.storage_diffs {
        for StorageEntry { key, value } in diff {
            let db_key = bincode::serialize(&(address, key, block_number)).unwrap();
            txn.upsert(storage_table, &db_key, value)?;
        }
    }
    Ok(())
}
