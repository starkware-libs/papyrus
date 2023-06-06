#[cfg(test)]
#[path = "compiled_class_test.rs"]
mod casm_test;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::ThinStateDiff;

use crate::db::{DbError, DbTransaction, TableHandle, TransactionKind, RW};
use crate::{MarkerKind, MarkersTable, StorageError, StorageResult, StorageTxn};

pub trait CasmStorageReader {
    /// Returns the Cairo assembly of a class given its Sierra class hash.
    fn get_casm(&self, class_hash: ClassHash) -> StorageResult<Option<CasmContractClass>>;
    fn get_compiled_class_marker(&self) -> StorageResult<BlockNumber>;
}

pub trait CasmStorageWriter
where
    Self: Sized,
{
    /// Stores the Cairo assembly of a class, mapped to its class hash.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_casm(self, class_hash: ClassHash, casm: &CasmContractClass) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> CasmStorageReader for StorageTxn<'env, Mode> {
    fn get_casm(&self, class_hash: ClassHash) -> StorageResult<Option<CasmContractClass>> {
        let casm_table = self.txn.open_table(&self.tables.casms)?;
        Ok(casm_table.get(&self.txn, &class_hash)?)
    }

    fn get_compiled_class_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::CompiledClass)?.unwrap_or_default())
    }
}

impl<'env> CasmStorageWriter for StorageTxn<'env, RW> {
    fn append_casm(self, class_hash: ClassHash, casm: &CasmContractClass) -> StorageResult<Self> {
        let casm_table = self.txn.open_table(&self.tables.casms)?;
        let markers_table = self.txn.open_table(&self.tables.markers)?;
        let state_diff_table = self.txn.open_table(&self.tables.state_diffs)?;
        casm_table.insert(&self.txn, &class_hash, casm).map_err(|err| {
            if matches!(err, DbError::Inner(libmdbx::Error::KeyExist)) {
                StorageError::CompiledClassReWrite { class_hash }
            } else {
                StorageError::from(err)
            }
        })?;
        update_marker(&self.txn, &markers_table, &state_diff_table, &class_hash)?;
        Ok(self)
    }
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    state_diffs_table: &'env TableHandle<'_, BlockNumber, ThinStateDiff>,
    class_hash: &ClassHash,
) -> StorageResult<()> {
    // The marker needs to update if we reached the last class from the state diff. We can continue
    // advancing it if the next blocks don't have declared classes.
    let mut block_number = markers_table.get(txn, &MarkerKind::CompiledClass)?.unwrap_or_default();
    loop {
        let Some(state_diff) = state_diffs_table
            .get(txn, &block_number)? else {break;};
        if let Some((last_class_hash, _)) = state_diff.declared_classes.last() {
            // Not the last class in the state diff, keep the current marker.
            if last_class_hash != class_hash {
                break;
            }
        }
        block_number = block_number.next();
        markers_table.upsert(txn, &MarkerKind::CompiledClass, &block_number)?;
    }
    Ok(())
}
