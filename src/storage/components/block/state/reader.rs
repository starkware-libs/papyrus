use libmdbx::RO;

use crate::{
    starknet::{
        BlockNumber, ClassHash, ContractAddress, IndexedDeployedContract, StarkFelt, StorageKey,
    },
    storage::{
        components::{block::BlockStorageResult, BlockStorageReader},
        db::{DbTransaction, TableHandle},
    },
};

// A helper object to get a StateReader.
// StateReader holds the open tables, which reference the txn. The can't be in the same struct -
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
        block_number: BlockNumber,
        address: &ContractAddress,
    ) -> BlockStorageResult<Option<ClassHash>> {
        let key = bincode::serialize(address).unwrap();
        let value = self
            .txn
            .get::<IndexedDeployedContract>(&self.contracts_table, &key)?;
        if let Some(value) = value {
            if block_number > value.block_number {
                return Ok(Some(value.class_hash));
            }
        }
        Ok(None)
    }
    pub fn get_storage_at(
        &self,
        block_number: BlockNumber,
        address: &ContractAddress,
        key: &StorageKey,
    ) -> BlockStorageResult<StarkFelt> {
        let db_key = bincode::serialize(&(address, key, block_number)).unwrap();
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
                    return Ok(StarkFelt::default());
                };
                Ok(value)
            }
        }
    }
}
