//! Interface for handling data related to Starknet [classes (Cairo 1)](https://docs.rs/starknet_api/latest/starknet_api/state/struct.ContractClass.html) and [deprecated classes (Cairo 0)](https://docs.rs/starknet_api/latest/starknet_api/deprecated_contract_class/struct.ContractClass.html).
//!
//! Import [`ClassStorageReader`] and [`ClassStorageWriter`] to read and write data related to
//! classes using a [`StorageTxn`].
//!
//! Note that the state diff needs to be written before writing
//! classes and the written classes' hashes should be the same as in the state diff.
//! # Example
//! ```
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! use indexmap::indexmap;
//! use papyrus_storage::class::{ClassStorageReader, ClassStorageWriter};
//! use papyrus_storage::open_storage;
//! use papyrus_storage::state::StateStorageWriter;
//! use starknet_api::block::BlockNumber;
//! use starknet_api::core::{ClassHash, CompiledClassHash};
//! use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
//! use starknet_api::hash::StarkHash;
//! use starknet_api::state::{ContractClass, ThinStateDiff};
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId("SN_MAIN".to_owned()),
//! #     enforce_file_exists: false,
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! # };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let class_hash = ClassHash::default();
//! let class = ContractClass::default();
//! let deprecated_class_hash = ClassHash(StarkHash::ONE);
//! let deprecated_class = DeprecatedContractClass::default();
//! let (reader, mut writer) = open_storage(storage_config)?;
//! writer
//!     .begin_rw_txn()? // Start a RW transaction.
//!     .append_thin_state_diff(
//!         BlockNumber(0),
//!         ThinStateDiff {
//!             declared_classes: indexmap! { class_hash => CompiledClassHash::default() },
//!             deprecated_declared_classes: vec![deprecated_class_hash],
//!             ..Default::default()
//!         },
//!     )?    // Append a state diff.
//!     .append_classes(
//!         BlockNumber(0),
//!         &vec![(class_hash, &class)],
//!         &vec![(deprecated_class_hash, &deprecated_class)],
//!     )? // Append all classes of block no. 0.
//!     .commit()?; // Commit the transaction.
//!
//! let written_class = reader.begin_ro_txn()?.get_class(&class_hash)?;
//! assert_eq!(written_class, Some(class));
//!
//! let written_deprecated_class =
//!     reader.begin_ro_txn()?.get_deprecated_class(&ClassHash(StarkHash::ONE))?;
//! assert_eq!(written_deprecated_class, Some(deprecated_class));
//! # Ok::<(), papyrus_storage::StorageError>(())
//! ```

#[cfg(test)]
#[path = "class_test.rs"]
mod class_test;

use std::collections::HashSet;

use papyrus_proc_macros::latency_histogram;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, ThinStateDiff};

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::state::{
    DeclaredClassesTable,
    DeprecatedDeclaredClassesTable,
    FileOffsetTable,
    StateStorageReader,
};
use crate::{
    DbTransaction,
    FileHandlers,
    IndexedDeprecatedContractClass,
    MarkerKind,
    OffsetKind,
    StorageError,
    StorageResult,
    StorageTxn,
};

/// Interface for reading data related to classes or deprecated classes.
pub trait ClassStorageReader {
    /// Returns the Cairo 1 class with the given hash.
    fn get_class(&self, class_hash: &ClassHash) -> StorageResult<Option<ContractClass>>;

    /// Returns the Cairo 0 class with the given hash.
    fn get_deprecated_class(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<DeprecatedContractClass>>;

    /// The block marker is the first block number that we don't have all of its classes.
    fn get_class_marker(&self) -> StorageResult<BlockNumber>;
}

/// Interface for writing data related to classes or deprecated classes.
pub trait ClassStorageWriter
where
    Self: Sized,
{
    /// Stores the classes declared in a block.
    ///
    /// Returns error if the hashes aren't identical to all the classes declared in the given block.
    ///
    /// Note: This function needs to be called for each block, even if there are no classes or
    /// deprecated classes declared in that block
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn append_classes(
        self,
        block_number: BlockNumber,
        classes: &[(ClassHash, &ContractClass)],
        deprecated_classes: &[(ClassHash, &DeprecatedContractClass)],
    ) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> ClassStorageReader for StorageTxn<'env, Mode> {
    fn get_class(&self, class_hash: &ClassHash) -> StorageResult<Option<ContractClass>> {
        let declared_classes_table = self.open_table(&self.tables.declared_classes)?;
        let contract_class_location = declared_classes_table.get(&self.txn, class_hash)?;
        contract_class_location
            .map(|location| self.file_handlers.get_contract_class_unchecked(location))
            .transpose()
    }

    fn get_deprecated_class(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<DeprecatedContractClass>> {
        let deprecated_declared_classes_table =
            self.open_table(&self.tables.deprecated_declared_classes)?;
        let deprecated_contract_class_location =
            deprecated_declared_classes_table.get(&self.txn, class_hash)?;
        deprecated_contract_class_location
            .map(|value| {
                self.file_handlers.get_deprecated_contract_class_unchecked(value.location_in_file)
            })
            .transpose()
    }

    fn get_class_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Class)?.unwrap_or_default())
    }
}

impl<'env> ClassStorageWriter for StorageTxn<'env, RW> {
    #[latency_histogram("storage_append_classes_latency_seconds")]
    fn append_classes(
        self,
        block_number: BlockNumber,
        classes: &[(ClassHash, &ContractClass)],
        deprecated_classes: &[(ClassHash, &DeprecatedContractClass)],
    ) -> StorageResult<Self> {
        let declared_classes_table = self.open_table(&self.tables.declared_classes)?;
        let deprecated_declared_classes_table =
            self.open_table(&self.tables.deprecated_declared_classes)?;
        let file_offset_table = self.txn.open_table(&self.tables.file_offsets)?;
        let markers_table = self.open_table(&self.tables.markers)?;

        let marker_block_number =
            markers_table.get(&self.txn, &MarkerKind::Class)?.unwrap_or_default();
        if block_number != marker_block_number {
            return Err(StorageError::MarkerMismatch {
                expected: marker_block_number,
                found: block_number,
            });
        }

        let state_diff =
            self.get_state_diff(block_number)?.ok_or(StorageError::MarkersUnorderingAttempt {
                surpassing_marker_kind: MarkerKind::Class,
                surpassed_marker_kind: MarkerKind::State,
            })?;
        validate_classes_identical_to_state_diff(
            &state_diff,
            classes,
            deprecated_classes,
            block_number,
        )?;

        write_classes(
            classes,
            &self.txn,
            &declared_classes_table,
            &self.file_handlers,
            &file_offset_table,
        )?;

        write_deprecated_classes(
            deprecated_classes,
            &self.txn,
            block_number,
            &deprecated_declared_classes_table,
            &self.file_handlers,
            &file_offset_table,
        )?;

        markers_table.upsert(&self.txn, &MarkerKind::Class, &block_number.unchecked_next())?;

        Ok(self)
    }
}

fn write_classes<'env>(
    classes: &[(ClassHash, &ContractClass)],
    txn: &DbTransaction<'env, RW>,
    declared_classes_table: &'env DeclaredClassesTable<'env>,
    file_handlers: &FileHandlers<RW>,
    file_offset_table: &'env FileOffsetTable<'env>,
) -> StorageResult<()> {
    for (class_hash, contract_class) in classes {
        let location = file_handlers.append_contract_class(contract_class);
        declared_classes_table.insert(txn, class_hash, &location)?;
        file_offset_table.upsert(txn, &OffsetKind::ContractClass, &location.next_offset())?;
    }
    Ok(())
}

fn write_deprecated_classes<'env>(
    deprecated_classes: &[(ClassHash, &DeprecatedContractClass)],
    txn: &DbTransaction<'env, RW>,
    block_number: BlockNumber,
    deprecated_declared_classes_table: &'env DeprecatedDeclaredClassesTable<'env>,
    file_handlers: &FileHandlers<RW>,
    file_offset_table: &'env FileOffsetTable<'env>,
) -> StorageResult<()> {
    for (class_hash, deprecated_contract_class) in deprecated_classes {
        if deprecated_declared_classes_table.get(txn, class_hash)?.is_some() {
            continue;
        }
        let location = file_handlers.append_deprecated_contract_class(deprecated_contract_class);
        let value = IndexedDeprecatedContractClass { block_number, location_in_file: location };
        file_offset_table.upsert(
            txn,
            &OffsetKind::DeprecatedContractClass,
            &location.next_offset(),
        )?;
        deprecated_declared_classes_table.insert(txn, class_hash, &value)?;
    }
    Ok(())
}

fn validate_classes_identical_to_state_diff(
    state_diff: &ThinStateDiff,
    classes: &[(ClassHash, &ContractClass)],
    deprecated_classes: &[(ClassHash, &DeprecatedContractClass)],
    block_number: BlockNumber,
) -> StorageResult<()> {
    if classes.iter().map(|(class_hash, _class)| class_hash).collect::<HashSet<_>>()
        != state_diff.declared_classes.keys().collect::<HashSet<_>>()
    {
        return Err(StorageError::ClassesStateDiffMismatch { block_number });
    }
    if deprecated_classes
        .iter()
        .map(|(class_hash, _deprecated_class)| class_hash)
        .collect::<HashSet<_>>()
        != state_diff.deprecated_declared_classes.iter().collect::<HashSet<_>>()
    {
        return Err(StorageError::ClassesStateDiffMismatch { block_number });
    }
    Ok(())
}
