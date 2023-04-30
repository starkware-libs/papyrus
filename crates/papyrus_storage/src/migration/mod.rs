use tracing::{debug, info, instrument};

use crate::db::{open_env, DbConfig, DbReader, DbWriter, TableIdentifier};
use crate::version::{Version, VERSION_KEY};
use crate::{StorageResult, STORAGE_VERSION};

#[cfg(test)]
#[path = "migration_test.rs"]
mod migration_test;

#[derive(thiserror::Error, Debug)]
pub enum StorageMigrationError {
    #[error(
        "DB version is not supported. Supported versions: up to {crate_version:?}, got version \
         {db_version:?}."
    )]
    UnsupportedDbVersion { crate_version: Version, db_version: Version },
}

fn get_db_version(
    db_reader: &DbReader,
    db_writer: &mut DbWriter,
) -> StorageResult<Option<Version>> {
    let version_table_id: TableIdentifier<String, Version> =
        db_writer.create_table("storage_version")?;
    let txn = db_reader.begin_ro_txn()?;
    let version_table_handle = txn.open_table(&version_table_id)?;
    let db_version = version_table_handle.get(&txn, &VERSION_KEY.to_string())?;
    Ok(db_version)
}

// Add version to the DB.
#[instrument(skip(db_writer), level = "debug", err)]
fn to_v0(db_writer: &mut DbWriter) -> StorageResult<()> {
    let version_table_id: TableIdentifier<String, Version> =
        db_writer.create_table("storage_version")?;
    let txn = db_writer.begin_rw_txn()?;
    txn.open_table(&version_table_id)?.insert(&txn, &VERSION_KEY.to_string(), &Version(0))?;
    txn.commit()?;
    Ok(())
}

#[instrument(skip(db_config), level = "debug", err)]
pub fn migrate_db(db_config: &DbConfig) -> StorageResult<()> {
    debug!("Beginning storage migration.");
    let (db_reader, mut db_writer) = open_env(db_config.clone())?;
    loop {
        let db_version = get_db_version(&db_reader, &mut db_writer)?;
        debug!(version=?db_version, "Upgrading version.");
        match db_version {
            // Upgrade storage version.
            None => to_v0(&mut db_writer)?,
            // Storage is up to date.
            Some(STORAGE_VERSION) => {
                info!(%STORAGE_VERSION, "Finished storage migration.");
                return StorageResult::Ok(());
            }
            Some(version) if version > STORAGE_VERSION => {
                return StorageResult::Err(
                    StorageMigrationError::UnsupportedDbVersion {
                        crate_version: STORAGE_VERSION,
                        db_version: version,
                    }
                    .into(),
                );
            }
            _ => unreachable!(),
        }
        debug!(version=?db_version, "Finished upgrading version.");
    }
}
