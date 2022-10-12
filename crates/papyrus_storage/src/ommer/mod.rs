use starknet_api::{BlockHash, BlockHeader, DeclaredContract};

#[cfg(test)]
#[path = "ommer_test.rs"]
mod ommer_test;

use crate::db::RW;
use crate::{StorageResult, StorageTxn, ThinStateDiff};

pub trait OmmerStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        thin_state_diff: &ThinStateDiff,
        declared_classes: &[DeclaredContract],
    ) -> StorageResult<Self>;

    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn insert_ommer_header(
        self,
        block_hash: BlockHash,
        header: &BlockHeader,
    ) -> StorageResult<Self>;
}

impl<'env> OmmerStorageWriter for StorageTxn<'env, RW> {
    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        thin_state_diff: &ThinStateDiff,
        declared_classes: &[DeclaredContract],
    ) -> StorageResult<Self> {
        let state_diffs_table = self.txn.open_table(&self.tables.ommer_state_diffs)?;
        let declared_classes_table = self.txn.open_table(&self.tables.ommer_declared_classes)?;
        let deployed_contracts_table =
            self.txn.open_table(&self.tables.ommer_deployed_contracts)?;
        let storage_table = self.txn.open_table(&self.tables.ommer_contract_storage)?;
        let nonces_table = self.txn.open_table(&self.tables.ommer_nonces)?;

        state_diffs_table.insert(&self.txn, &block_hash, thin_state_diff)?;

        for declared_class in declared_classes {
            let key = (block_hash, declared_class.class_hash);
            let value = declared_class.contract_class.to_byte_vec();
            declared_classes_table.insert(&self.txn, &key, &value)?;
        }

        for deployed_contract in thin_state_diff.deployed_contracts() {
            let key = (deployed_contract.address, block_hash);
            let value = deployed_contract.class_hash;
            deployed_contracts_table.insert(&self.txn, &key, &value)?;
        }

        for storage_diff in thin_state_diff.storage_diffs() {
            for storage_entry in &storage_diff.storage_entries {
                let key = (storage_diff.address, storage_entry.key.clone(), block_hash);
                let value = storage_entry.value;
                storage_table.insert(&self.txn, &key, &value)?;
            }
        }

        for contract_nonce in thin_state_diff.nonces() {
            let key = (contract_nonce.contract_address, block_hash);
            let value = contract_nonce.nonce;
            nonces_table.insert(&self.txn, &key, &value)?;
        }

        Ok(self)
    }

    fn insert_ommer_header(
        self,
        block_hash: BlockHash,
        header: &BlockHeader,
    ) -> StorageResult<Self> {
        let headers_table = self.txn.open_table(&self.tables.ommer_headers)?;
        headers_table.insert(&self.txn, &block_hash, header)?;

        Ok(self)
    }
}
