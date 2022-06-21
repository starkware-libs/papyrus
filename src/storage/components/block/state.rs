#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use libmdbx::{RO, RW};

use super::{BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter};

use crate::starknet::{
    BlockNumber, ClassHash, ContractAddress, StarkFelt, StateDiffBackward, StateDiffForward,
    StorageDiff, StorageEntry, StorageKey,
};
use crate::storage::db::{DbTransaction, TableHandle};

// Constants.
const STATE_MARKER_KEY: &[u8] = b"state";

pub trait StateStorageReader {
    fn get_state_marker(&self) -> BlockStorageResult<BlockNumber>;
    fn get_state_diff(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<StateDiffBackward>>;
    fn get_state_reader_txn(&self) -> BlockStorageResult<StateReaderTxn<'_>>;
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
    ) -> BlockStorageResult<Option<StateDiffBackward>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;
        let state_diff = txn.get::<StateDiffBackward>(
            &state_diffs_table,
            &bincode::serialize(&block_number).unwrap(),
        )?;
        Ok(state_diff)
    }
    fn get_state_reader_txn(&self) -> BlockStorageResult<StateReaderTxn<'_>> {
        let txn = self.db_reader.begin_ro_txn()?;
        Ok(StateReaderTxn { reader: self, txn })
    }
}

// A helper object to get a StateReader.
pub struct StateReaderTxn<'env> {
    reader: &'env BlockStorageReader,
    txn: DbTransaction<'env, RO>,
}
#[allow(dead_code)]
impl<'env> StateReaderTxn<'env> {
    fn get_state_reader(&self) -> BlockStorageResult<StateReader<'_, '_>> {
        let txn = &self.txn;

        let contracts_table = txn.open_table(&self.reader.tables.contracts)?;
        let storage_table = txn.open_table(&self.reader.tables.contract_storage)?;

        Ok(StateReader {
            txn,
            contracts_table,
            storage_table,
        })
    }
}
// Represents a single coherent state at a single point in time,
pub struct StateReader<'env: 'txn, 'txn> {
    txn: &'txn DbTransaction<'env, RO>,
    contracts_table: TableHandle<'txn>,
    storage_table: TableHandle<'txn>,
}
#[allow(dead_code)]
impl<'env, 'txn> StateReader<'env, 'txn> {
    fn get_class_hash_at(&self, address: ContractAddress) -> BlockStorageResult<Option<ClassHash>> {
        Ok(self
            .txn
            .get::<ClassHash>(&self.contracts_table, &address.0 .0)?)
    }
    fn get_storage_at(
        &self,
        address: ContractAddress,
        key: &StorageKey,
    ) -> BlockStorageResult<StarkFelt> {
        let db_key = [address.0 .0, key.0 .0].concat();
        Ok(self
            .txn
            .get::<StarkFelt>(&self.storage_table, &db_key)?
            .unwrap_or_default())
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
        let contracts_table = txn.open_table(&self.tables.contracts)?;
        let storage_table = txn.open_table(&self.tables.contract_storage)?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;

        update_marker(&txn, &markers_table, block_number)?;

        // Translate forward diff to backward diff.
        let deployed_contracts = translate_deployed_contracts(state_diff, &txn, &contracts_table)?;
        let storage_diffs = translate_storage_diffs(state_diff, &txn, &storage_table)?;

        // Write state diff.
        txn.insert(
            &state_diffs_table,
            &bincode::serialize(&block_number).unwrap(),
            &StateDiffBackward {
                deployed_contracts,
                storage_diffs,
            },
        )?;

        // Finalize.
        txn.commit()?;
        Ok(())
    }
}

fn update_marker(
    txn: &DbTransaction<RW>,
    markers_table: &TableHandle,
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

fn translate_deployed_contracts(
    state_diff: &StateDiffForward,
    txn: &DbTransaction<RW>,
    contracts_table: &TableHandle,
) -> BlockStorageResult<Vec<ContractAddress>> {
    let mut deployed_contracts: Vec<ContractAddress> = Vec::new();
    deployed_contracts.reserve(state_diff.deployed_contracts.len());
    for deployed_contract in &state_diff.deployed_contracts {
        deployed_contracts.push(deployed_contract.address);
        txn.insert(
            contracts_table,
            &deployed_contract.address.0 .0,
            &deployed_contract.class_hash,
        )?;
    }
    Ok(deployed_contracts)
}
fn translate_storage_diffs(
    state_diff: &StateDiffForward,
    txn: &DbTransaction<RW>,
    storage_table: &TableHandle,
) -> BlockStorageResult<Vec<StorageDiff>> {
    let mut storage_diffs: Vec<StorageDiff> = Vec::new();
    storage_diffs.reserve(state_diff.storage_diffs.len());
    for StorageDiff { address, diff } in &state_diff.storage_diffs {
        let mut backward_contract_diffs: Vec<StorageEntry> = Vec::new();
        backward_contract_diffs.reserve(diff.len());
        for StorageEntry { addr, value } in diff {
            let db_key = [address.0 .0, addr.0 .0].concat();
            let prev_value = txn
                .get::<StarkFelt>(storage_table, &db_key)?
                .unwrap_or_default();
            backward_contract_diffs.push(StorageEntry {
                addr: addr.clone(),
                value: prev_value,
            });

            // Update DB.
            if value == &StarkFelt::default() {
                txn.delete::<StarkFelt>(storage_table, &db_key)?;
            } else {
                txn.upsert(storage_table, &db_key, value)?;
            }
        }
        storage_diffs.push(StorageDiff {
            address: *address,
            diff: backward_contract_diffs,
        });
    }
    Ok(storage_diffs)
}
