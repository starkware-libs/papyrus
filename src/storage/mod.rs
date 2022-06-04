mod api;
mod db;
mod storage_impl;
#[cfg(test)]
mod storage_test;

pub use self::storage_impl::{create_store_access, SNStorageReader, SNStorageWriter};

pub use self::api::{StorageError, StorageReader, StorageWriter};
