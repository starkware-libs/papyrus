use starknet_api::{BlockNumber, ClassHash, ContractAddress, StarkFelt, StateNumber, StorageKey};

use super::{ContractStorageTable, ContractsTable};
use crate::storage::components::block::BlockStorageResult;
use crate::storage::components::BlockStorageTxn;
use crate::storage::db::{DbTransaction, TransactionKind};

// Represents a single coherent state at a single point in time,
pub struct StateReader<'env, Mode: TransactionKind> {
    txn: &'env DbTransaction<'env, Mode>,
    contracts_table: ContractsTable<'env>,
    storage_table: ContractStorageTable<'env>,
}
#[allow(dead_code)]
impl<'env, Mode: TransactionKind> StateReader<'env, Mode> {
    pub fn new(txn: &'env BlockStorageTxn<'env, Mode>) -> BlockStorageResult<Self> {
        let contracts_table = txn.txn.open_table(&txn.tables.contracts)?;
        let storage_table = txn.txn.open_table(&txn.tables.contract_storage)?;
        Ok(StateReader { txn: &txn.txn, contracts_table, storage_table })
    }
    pub fn get_class_hash_at(
        &self,
        state_number: StateNumber,
        address: &ContractAddress,
    ) -> BlockStorageResult<Option<ClassHash>> {
        let value = self.contracts_table.get(self.txn, address)?;
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
        let db_key = (*address, key.clone(), first_irrelevant_block);
        // Find the previous db item.
        let mut cursor = self.storage_table.cursor(self.txn)?;
        cursor.lower_bound(&db_key)?;
        let res = cursor.prev()?;
        match res {
            None => Ok(StarkFelt::default()),
            Some(((got_address, got_key, _got_block_number), value)) => {
                if got_address != *address || got_key != *key {
                    // The previous item belongs to different key, which means there is no
                    // previous state diff for this item.
                    return Ok(StarkFelt::default());
                };
                // The previous db item indeed belongs to this address and key.
                Ok(value)
            }
        }
    }
}
