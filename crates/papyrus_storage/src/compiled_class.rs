#[cfg(test)]
#[path = "compiled_class_test.rs"]
mod casm_test;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;

use crate::db::{DbError, TransactionKind, RW};
use crate::{StorageError, StorageResult, StorageTxn};

pub trait CasmStorageReader {
    /// Returns the Cairo assembly of a class given its Sierra class hash.
    fn get_casm(&self, class_hash: &ClassHash) -> StorageResult<Option<CasmContractClass>>;
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
    fn get_casm(&self, class_hash: &ClassHash) -> StorageResult<Option<CasmContractClass>> {
        let casm_table = self.txn.open_table(&self.tables.casms)?;
        Ok(casm_table.get(&self.txn, class_hash)?)
    }
}

impl<'env> CasmStorageWriter for StorageTxn<'env, RW> {
    fn append_casm(self, class_hash: ClassHash, casm: &CasmContractClass) -> StorageResult<Self> {
        let casm_table = self.txn.open_table(&self.tables.casms)?;
        casm_table.insert(&self.txn, &class_hash, casm).map_err(|err| {
            if matches!(err, DbError::Inner(libmdbx::Error::KeyExist)) {
                StorageError::CompiledClassReWrite { class_hash }
            } else {
                StorageError::from(err)
            }
        })?;

        Ok(self)
    }
}
