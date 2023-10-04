#![allow(clippy::unwrap_used)]
//! Test utilities for the storage crate users.

use starknet_api::core::ChainId;
use tempfile::{tempdir, TempDir};

use crate::db::DbConfig;
use crate::mmap_file::MmapFileConfig;
use crate::{open_storage, StorageConfig, StorageReader, StorageScope, StorageWriter};

/// Returns a db config and the temporary directory that holds this db.
/// The TempDir object is returned as a handler for the lifetime of this object (the temp
/// directory), thus make sure the directory won't be destroyed. The caller should propagate the
/// TempDir object until it is no longer needed. When the TempDir object is dropped, the directory
/// is deleted.
pub fn get_test_config(storage_scope: Option<StorageScope>) -> (StorageConfig, TempDir) {
    let storage_scope = storage_scope.unwrap_or_default();
    let dir = tempdir().unwrap();
    println!("{dir:?}");
    (
        StorageConfig {
            db_config: DbConfig {
                path_prefix: dir.path().to_path_buf(),
                chain_id: ChainId("".to_owned()),
                min_size: 1 << 20,    // 1MB
                max_size: 1 << 35,    // 32GB
                growth_step: 1 << 26, // 64MB
            },
            scope: storage_scope,
            mmap_file_config: get_mmap_file_test_config(),
        },
        dir,
    )
}

/// Returns [`StorageReader`], [`StorageWriter`] and the temporary directory that holds a db for
/// testing purposes.
pub fn get_test_storage() -> ((StorageReader, StorageWriter), TempDir) {
    let (config, temp_dir) = get_test_config(None);
    ((open_storage(config).unwrap()), temp_dir)
}

/// Returns a [`MmapFileConfig`] for testing purposes.
pub fn get_mmap_file_test_config() -> MmapFileConfig {
    MmapFileConfig {
        max_size: 1 << 24,       // 16MB
        growth_step: 1 << 20,    // 1MB
        max_object_size: 1 << 8, // 256B
    }
}

/// Returns [`StorageReader`], [`StorageWriter`] that configured by the given [`StorageScope`] and
/// the temporary directory that holds a db for testing purposes.
pub fn get_test_storage_by_scope(
    storage_scope: StorageScope,
) -> ((StorageReader, StorageWriter), TempDir) {
    let (config, temp_dir) = get_test_config(Some(storage_scope));
    ((open_storage(config).unwrap()), temp_dir)
}
