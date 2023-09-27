#[cfg(test)]
#[path = "version_test.rs"]
mod version_test;

use std::fmt::Display;

use crate::db::{TransactionKind, RW};
use crate::{StorageError, StorageResult, StorageTxn};

const VERSION_KEY: &str = "storage_version";

#[derive(Clone, Debug, Default, Eq, PartialEq, PartialOrd)]
pub struct Version(pub u32);

#[derive(thiserror::Error, Debug)]
pub enum StorageVersionError {
    #[error(
        "Storage crate version {crate_version} is inconsistent with DB version {storage_version}."
    )]
    InconsistentStorageVersion { crate_version: Version, storage_version: Version },
    #[error(
        "Trying to set a DB version {crate_version:} which is not higher that the existing one \
         {storage_version}."
    )]
    SetLowerVersion { crate_version: Version, storage_version: Version },
}

pub trait VersionStorageReader {
    fn get_version(&self) -> StorageResult<Option<Version>>;
}

pub trait VersionStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn set_version(self, version: &Version) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> VersionStorageReader for StorageTxn<'env, Mode> {
    fn get_version(&self) -> StorageResult<Option<Version>> {
        let version_table = self.open_table(&self.tables.storage_version)?;
        Ok(version_table.get(&self.txn, &VERSION_KEY.to_string())?)
    }
}

impl<'env> VersionStorageWriter for StorageTxn<'env, RW> {
    fn set_version(self, version: &Version) -> StorageResult<Self> {
        let version_table = self.open_table(&self.tables.storage_version)?;
        if let Some(current_storage_version) = self.get_version()? {
            if current_storage_version >= *version {
                return Err(StorageError::StorageVersionInconcistency(
                    StorageVersionError::SetLowerVersion {
                        crate_version: version.clone(),
                        storage_version: current_storage_version,
                    },
                ));
            };
        }
        version_table.upsert(&self.txn, &VERSION_KEY.to_string(), version)?;
        Ok(self)
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let version = self.0.to_string();
        write!(f, "{version}")
    }
}
