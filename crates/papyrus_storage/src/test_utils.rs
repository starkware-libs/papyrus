#![allow(clippy::unwrap_used)]
//! Test utilities for the storage crate users.

use starknet_api::core::ChainId;
use tempfile::{tempdir, TempDir};

use crate::db::DbConfig;
use crate::{open_storage, StorageReader, StorageWriter};

/// Returns a db config and the temporary directory that holds this db.
/// The TempDir object is returned as a handler for the lifetime of this object (the temp
/// directory), thus make sure the directory won't be destroyed. The caller should propagate the
/// TempDir object until it is no longer needed. When the TempDir object is dropped, the directory
/// is deleted.
pub fn get_test_config() -> (DbConfig, TempDir) {
    let dir = tempdir().unwrap();
    println!("{dir:?}");
    (
        DbConfig {
            path_prefix: dir.path().to_path_buf(),
            chain_id: ChainId("".to_owned()),
            min_size: 1 << 20,    // 1MB
            max_size: 1 << 35,    // 32GB
            growth_step: 1 << 26, // 64MB
        },
        dir,
    )
}

/// Returns [`StorageReader`], [`StorageWriter`] and the temporary directory that holds a db for
/// testing purposes.
pub fn get_test_storage() -> ((StorageReader, StorageWriter), TempDir) {
    let (config, temp_dir) = get_test_config();
    ((open_storage(config).unwrap()), temp_dir)
}
