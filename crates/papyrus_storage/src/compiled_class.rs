#[cfg(test)]
#[path = "compiled_class_test.rs"]
mod compiled_class_test;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;

use crate::db::{DbError, TransactionKind, RW};
use crate::{StorageError, StorageResult, StorageTxn};

pub trait CompiledClassStorageReader {
    /// Returns the Cairo assembly of a class given its class hash.
    fn get_compiled_class(&self, class_hash: ClassHash)
    -> StorageResult<Option<CasmContractClass>>;
}

pub trait CompiledClassStorageWriter
where
    Self: Sized,
{
    /// Stores the Cairo assembly of a class, mapped to its class hash.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_compiled_class(
        self,
        class_hash: ClassHash,
        casm: &CasmContractClass,
    ) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> CompiledClassStorageReader for StorageTxn<'env, Mode> {
    fn get_compiled_class(
        &self,
        class_hash: ClassHash,
    ) -> StorageResult<Option<CasmContractClass>> {
        let casm_table = self.txn.open_table(&self.tables.compiled_classes)?;
        Ok(casm_table.get(&self.txn, &class_hash)?)
    }
}

impl<'env> CompiledClassStorageWriter for StorageTxn<'env, RW> {
    fn append_compiled_class(
        self,
        class_hash: ClassHash,
        casm: &CasmContractClass,
    ) -> StorageResult<Self> {
        let casm_table = self.txn.open_table(&self.tables.compiled_classes)?;
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
