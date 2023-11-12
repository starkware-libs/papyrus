//! Interface for handling data related to Starknet [compiled classes (Cairo assembly, or CASM)](https://docs.rs/cairo-lang-starknet/latest/cairo_lang_starknet/casm_contract_class/struct.CasmContractClass.html).
//!
//! The compiled class is the result of compiling a Cairo program.
//! Import [`CasmStorageReader`] and [`CasmStorageWriter`] to read and write data related to the
//! compiled classes using a [`StorageTxn`].
//! # Example
//! ```
//! use papyrus_storage::open_storage;
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! use cairo_lang_starknet::casm_contract_class::CasmContractClass;
//! use papyrus_storage::compiled_class::{CasmStorageReader, CasmStorageWriter};
//! use starknet_api::core::ClassHash;
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId("SN_MAIN".to_owned()),
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! # };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let (reader, mut writer) = open_storage(storage_config)?;
//! writer
//!     .begin_rw_txn()?                                                    // Start a RW transaction.
//!     .append_casm(&ClassHash::default(), &CasmContractClass::default())? // Append a compiled class.
//!     .commit()?;                                                         // Commit the transaction.
//! let casm = reader.begin_ro_txn()?.get_casm(&ClassHash::default())?;
//! assert_eq!(casm, Some(CasmContractClass::default()));
//! # Ok::<(), papyrus_storage::StorageError>(())
//! ```

#[cfg(test)]
#[path = "compiled_class_test.rs"]
mod casm_test;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use papyrus_proc_macros::latency_histogram;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;

use crate::db::{DbTransaction, TableHandle, TransactionKind, RW};
use crate::mmap_file::LocationInFile;
use crate::{FileHandlers, MarkerKind, MarkersTable, OffsetKind, StorageResult, StorageTxn};

/// Interface for reading data related to the compiled classes.
pub trait CasmStorageReader {
    /// Returns the Cairo assembly of a class given its Sierra class hash.
    fn get_casm(&self, class_hash: &ClassHash) -> StorageResult<Option<CasmContractClass>>;
    /// The block marker is the first block number that doesn't exist yet.
    ///
    /// Note: If the last blocks don't contain any declared classes, the marker will point at the
    /// block after the last block that had declared classes.
    fn get_compiled_class_marker(&self) -> StorageResult<BlockNumber>;
}

/// Interface for writing data related to the compiled classes.
pub trait CasmStorageWriter
where
    Self: Sized,
{
    /// Stores the Cairo assembly of a class, mapped to its class hash.
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_casm(self, class_hash: &ClassHash, casm: &CasmContractClass) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> CasmStorageReader for StorageTxn<'env, Mode> {
    fn get_casm(&self, class_hash: &ClassHash) -> StorageResult<Option<CasmContractClass>> {
        let casm_table = self.open_table(&self.tables.casms)?;
        let casm_location = casm_table.get(&self.txn, class_hash)?;
        casm_location.map(|location| self.file_handlers.get_casm_unchecked(location)).transpose()
    }

    fn get_compiled_class_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::CompiledClass)?.unwrap_or_default())
    }
}

impl<'env> CasmStorageWriter for StorageTxn<'env, RW> {
    #[latency_histogram("storage_append_casm_latency_seconds")]
    fn append_casm(self, class_hash: &ClassHash, casm: &CasmContractClass) -> StorageResult<Self> {
        let casm_table = self.open_table(&self.tables.casms)?;
        let markers_table = self.open_table(&self.tables.markers)?;
        let state_diff_table = self.open_table(&self.tables.state_diffs)?;
        let file_offset_table = self.txn.open_table(&self.tables.file_offsets)?;

        let location = self.file_handlers.append_casm(casm);
        casm_table.insert(&self.txn, class_hash, &location)?;
        file_offset_table.upsert(&self.txn, &OffsetKind::Casm, &location.next_offset())?;
        update_marker(
            &self.txn,
            &markers_table,
            &state_diff_table,
            self.file_handlers.clone(),
            class_hash,
        )?;
        Ok(self)
    }
}

fn update_marker<'env>(
    txn: &DbTransaction<'env, RW>,
    markers_table: &'env MarkersTable<'env>,
    state_diffs_table: &'env TableHandle<'_, BlockNumber, LocationInFile>,
    file_handlers: FileHandlers<RW>,
    class_hash: &ClassHash,
) -> StorageResult<()> {
    // The marker needs to update if we reached the last class from the state diff. We can continue
    // advancing it if the next blocks don't have declared classes.
    let mut block_number = markers_table.get(txn, &MarkerKind::CompiledClass)?.unwrap_or_default();
    loop {
        let Some(state_diff_location) = state_diffs_table.get(txn, &block_number)? else {
            break;
        };
        if let Some((last_class_hash, _)) = file_handlers
            .get_thin_state_diff_unchecked(state_diff_location)?
            .declared_classes
            .last()
        {
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
