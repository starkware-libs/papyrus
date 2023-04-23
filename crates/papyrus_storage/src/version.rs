#[cfg(test)]
#[path = "version_test.rs"]
mod version_test;

use std::fmt::Display;

use semver::{Version as SemVerVersion, VersionReq};

use crate::db::serialization::{StorageSerde, StorageSerdeError};
use crate::db::{TransactionKind, RW};
use crate::{StorageError, StorageResult, StorageTxn};

const VERSION_KEY: &str = "storage_version";

#[derive(Clone, Debug)]
pub struct Version(pub SemVerVersion);

impl Version {
    pub(crate) fn no_breaking_changes_since(&self, other: &Version) -> StorageResult<bool> {
        let req_string = match self.0.major {
            major if major == 0 => format!(">={}.{}", major, self.0.minor),
            major => format!(">={}", major),
        };
        let req = VersionReq::parse(req_string.as_str()).map_err(StorageVersionError::Semver)?;
        Ok(req.matches(&other.0))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum StorageVersionError {
    #[error(
        "Storage crate version {crate_version} is inconsistent with DB version {storage_version}."
    )]
    InconsistentStorageVersion { crate_version: Version, storage_version: Version },
    #[error(transparent)]
    Semver(#[from] semver::Error),
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
    fn set_version(self, version: Version) -> StorageResult<Self>;
}

impl<'env, Mode: TransactionKind> VersionStorageReader for StorageTxn<'env, Mode> {
    fn get_version(&self) -> StorageResult<Option<Version>> {
        let version_table = self.txn.open_table(&self.tables.storage_version)?;
        Ok(version_table.get(&self.txn, &VERSION_KEY.to_string())?)
    }
}

impl<'env> VersionStorageWriter for StorageTxn<'env, RW> {
    fn set_version(self, version: Version) -> StorageResult<Self> {
        let version_table = self.txn.open_table(&self.tables.storage_version)?;
        if let Some(current_storage_version) = self.get_version()? {
            if current_storage_version.0.ge(&version.0) {
                return Err(StorageError::StorageVersion(StorageVersionError::SetLowerVersion {
                    crate_version: version,
                    storage_version: current_storage_version,
                }));
            };
        }
        version_table.upsert(&self.txn, &VERSION_KEY.to_string(), &version)?;
        Ok(self)
    }
}

impl StorageSerde for Version {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.0.to_string().serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Version> {
        if let Ok(semver) = SemVerVersion::parse(String::deserialize_from(bytes)?.as_str()) {
            Some(Version(semver))
        } else {
            None
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let version = self.0.to_string();
        write!(f, "{version}")
    }
}

impl Default for Version {
    fn default() -> Self {
        Self(semver::Version::new(0, 0, 0))
    }
}
