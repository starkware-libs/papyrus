mod reader;
#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

use crate::starknet::{
    BlockNumber, ContractAddress, IndexedDeployedContract, StarkFelt, StateDiffForward,
    StorageDiff, StorageEntry, StorageKey,
};
use crate::storage::db::{DbError, DbTransaction, TableHandle, RW};

use super::{
    BlockStorageError, BlockStorageReader, BlockStorageResult, BlockStorageWriter, MarkerKind,
    MarkersTable,
};

use self::reader::StateReaderTxn;

pub type ContractsTable<'env> = TableHandle<'env, ContractAddress, IndexedDeployedContract>;
pub type ContractStorageTable<'env> =
    TableHandle<'env, (ContractAddress, StorageKey, BlockNumber), StarkFelt>;

// Structure of state data:
// * contracts_table: (contract_address) -> (block_num, class_hash).
//   Each entry specifies at which block was this contract deployed and with what class hash.
//   Note that each contract may only be deployed once, so we don't need to support multiple
//   entries per contract address.
// * storage_table: (contract_address, key, block_num) -> (value).
//   Specifies that at `block_num`, the `key` at `contract_address` was changed to `value`.
//   This structure let's us do quick lookup, since the database supports "Get the closet element
//   from  the left". Thus, to lookup the value at a specific block_number, we can search
//   (contract_address, key, block_num), and retrieve the closest from left, which should be
//   the latest update to the value before that block_num.

pub trait StateStorageReader {
    fn get_state_marker(&self) -> BlockStorageResult<BlockNumber>;
    fn get_state_diff(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<StateDiffForward>>;
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
        Ok(markers_table
            .get(&txn, &MarkerKind::State)?
            .unwrap_or_default())
    }
    fn get_state_diff(
        &self,
        block_number: BlockNumber,
    ) -> BlockStorageResult<Option<StateDiffForward>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;
        let state_diff = state_diffs_table.get(&txn, &block_number)?;
        Ok(state_diff)
    }
    fn get_state_reader_txn(&self) -> BlockStorageResult<StateReaderTxn<'_>> {
        let txn = self.db_reader.begin_ro_txn()?;
        Ok(StateReaderTxn { reader: self, txn })
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
        state_diffs_table.insert(&txn, &block_number, state_diff)?;
        // Write state.
        write_deployed_contracts(state_diff, &txn, block_number, &contracts_table)?;
        write_storage_diffs(state_diff, &txn, block_number, &storage_table)?;

        // Finalize.
        txn.commit()?;
        Ok(())
    }
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    block_number: BlockNumber,
) -> BlockStorageResult<()> {
    // Make sure marker is consistent.
    let state_marker = markers_table
        .get(txn, &MarkerKind::State)?
        .unwrap_or_default();
    if state_marker != block_number {
        return Err(BlockStorageError::MarkerMismatch {
            expected: state_marker,
            found: block_number,
        });
    };

    // Advance marker.
    markers_table.upsert(txn, &MarkerKind::State, &block_number.next())?;
    Ok(())
}

fn write_deployed_contracts<'env>(
    state_diff: &StateDiffForward,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    contracts_table: &'env ContractsTable<'env>,
) -> BlockStorageResult<()> {
    for deployed_contract in &state_diff.deployed_contracts {
        let class_hash = deployed_contract.class_hash;
        let value = IndexedDeployedContract {
            block_number,
            class_hash,
        };
        let res = contracts_table.insert(txn, &deployed_contract.address, &value);
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

fn write_storage_diffs<'env>(
    state_diff: &StateDiffForward,
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    storage_table: &'env ContractStorageTable<'env>,
) -> BlockStorageResult<()> {
    for StorageDiff { address, diff } in &state_diff.storage_diffs {
        for StorageEntry { key, value } in diff {
            storage_table.upsert(txn, &(*address, key.clone(), block_number), value)?;
        }
    }
    Ok(())
}
