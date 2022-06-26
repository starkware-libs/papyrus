use libmdbx::RO;

use crate::{
    starknet::{
        BlockNumber, ClassHash, ContractAddress, IndexedDeployedContract, StarkFelt, StateNumber,
        StorageKey,
    },
    storage::{
        components::{block::BlockStorageResult, BlockStorageReader},
        db::{DbTransaction, TableHandle},
    },
};

// Represents a single coherent state at a single point in time,
pub struct StateReader<'env, 'txn> {
    txn: &'txn DbTransaction<'env, RO>,
    contracts_table: TableHandle<'txn>,
    storage_table: TableHandle<'txn>,
}
#[allow(dead_code)]
impl<'env, 'txn> StateReader<'env, 'txn> {
    pub fn get_class_hash_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> BlockStorageResult<Option<ClassHash>> {
        let key = bincode::serialize(address).unwrap();
        let value = self
            .txn
            .get::<IndexedDeployedContract>(&self.contracts_table, &key)?;
        if let Some(value) = value {
            if state_number.is_after(value.block_number) {
                return Ok(Some(value.class_hash));
            }
        }
        Ok(None)
    }
    pub fn get_storage_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
        key: &StorageKey,
    ) -> BlockStorageResult<StarkFelt> {
        // The updates to the storage key are indexed by the block_number at which they occured.
        let first_irrelevant_block: BlockNumber = state_number.block_after();
        // The relevant update is the last update strictly before `first_irrelevant_block`.
        let db_key = bincode::serialize(&(address, key, first_irrelevant_block)).unwrap();
        // Find the previous db item.
        let res = self
            .txn
            .get_lower_item::<StarkFelt>(&self.storage_table, &db_key)?;
        match res {
            None => Ok(StarkFelt::default()),
            Some((got_db_key, value)) => {
                let (got_address, got_key, _got_block_number) =
                    bincode::deserialize::<(ContractAddress, StorageKey, BlockNumber)>(&got_db_key)
                        .unwrap();
                if got_address != *address || got_key != *key {
                    // The previous item belonds to different key, which means there is no
                    // previous state diff for this item.
                    return Ok(StarkFelt::default());
                };
                // The previous db item indeed belongs to this address and key.
                Ok(value)
            }
        }
    }
}

// A helper object to get a StateReader.
// StateReader holds the open tables, which reference the txn. They can't be in the same struct -
// that would be a self reference.
// Instead, one should hold the txn, and then open the tables in an inner lifetime.
pub struct StateReaderTxn<'env> {
    pub reader: &'env BlockStorageReader,
    pub txn: DbTransaction<'env, RO>,
}
#[allow(dead_code)]
impl<'env> StateReaderTxn<'env> {
    pub fn get_state_reader(&self) -> BlockStorageResult<StateReader<'_, '_>> {
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
