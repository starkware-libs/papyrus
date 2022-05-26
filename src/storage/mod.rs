mod api;
mod storage_impl;
#[cfg(test)]
mod storage_tests;

use std::sync::Arc;

use tokio::sync::Mutex;

use self::api::StorageError;
pub use self::api::{StarknetStorageReader, StarknetStorageWriter};
use self::storage_impl::{SNStorageReader, SNStorageWriter, TheDataStore};

/**
 * This is the function that's supposed to be called by the function that initializes
 * the store and wires it to relevant other modules.
 */
pub fn create_store_access() -> Result<(SNStorageReader, SNStorageWriter), StorageError> {
    //TODO: find a way to limit calls to this function

    let ds = TheDataStore::new();

    let m = Arc::new(Mutex::new(ds));

    let r = SNStorageReader { store: m.clone() };

    let w = SNStorageWriter { store: m.clone() };

    Ok((r, w))
}
