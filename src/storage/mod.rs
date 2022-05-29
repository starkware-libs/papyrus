mod api;
mod storage_impl;
#[cfg(test)]
mod storage_tests;

pub use self::storage_impl::create_store_access;

pub use self::api::{StorageReader, StorageWriter};
