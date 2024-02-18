use tracing::error;

use crate::db::serialization::{Migratable, StorageSerde, StorageSerdeError};
use crate::header::{StorageBlockHeader, StorageBlockHeaderV0};

impl Migratable for StorageBlockHeader {
    fn try_from_older_version(
        bytes: &mut impl std::io::Read,
        older_version: u8,
    ) -> Result<Self, StorageSerdeError> {
        if older_version != 0 {
            error!(
                "Unable to migrate stored header from version {} to current version.",
                older_version
            );
            return Err(StorageSerdeError::Migration);
        }
        let v0_header =
            StorageBlockHeaderV0::deserialize_from(bytes).ok_or(StorageSerdeError::Migration)?;
        Ok(v0_header.into())
    }
}
