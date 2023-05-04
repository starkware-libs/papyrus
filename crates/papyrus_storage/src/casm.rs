#[cfg(test)]
#[path = "casm_test.rs"]
mod casm_test;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;

use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

pub trait CasmStorageReader {
    fn get_casm(&self, class_hash: ClassHash) -> StorageResult<Option<CasmContractClass>>;
}

pub trait CasmStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_casm(self, class_hash: ClassHash, casm: &CasmContractClass) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> CasmStorageReader for StorageTxn<'env, Mode> {
    fn get_casm(&self, class_hash: ClassHash) -> StorageResult<Option<CasmContractClass>> {
        let casm_table = self.txn.open_table(&self.tables.casms)?;
        Ok(casm_table.get(&self.txn, &class_hash)?)
    }
}

impl<'env> CasmStorageWriter for StorageTxn<'env, RW> {
    fn append_casm(self, class_hash: ClassHash, casm: &CasmContractClass) -> StorageResult<Self> {
        let casm_table = self.txn.open_table(&self.tables.casms)?;
        casm_table.insert(&self.txn, &class_hash, casm)?;

        Ok(self)
    }
}
