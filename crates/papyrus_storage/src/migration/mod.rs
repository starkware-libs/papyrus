use tracing::{debug, info, instrument};

use crate::db::serialization::StorageSerde;
use crate::db::{DbError, DbWriteTransaction, DbWriter, TableIdentifier};
use crate::version::{Version, VERSION_KEY};
use crate::STORAGE_VERSION;

#[cfg(test)]
#[path = "migration_simulation.rs"]
mod migration_simulation;
#[cfg(test)]
#[path = "migration_test.rs"]
mod migration_test;

#[derive(thiserror::Error, Debug)]
pub enum StorageMigrationError {
    #[error(transparent)]
    Inner(#[from] libmdbx::Error),
    #[error(
        "DB version is not supported. Supported versions: up to {crate_version:?}, got version \
         {db_version:?}."
    )]
    UnsupportedDbVersion { crate_version: Version, db_version: Version },
    #[error(transparent)]
    DB(#[from] DbError),
}

pub type StorageMigrationResult<V> = Result<V, StorageMigrationError>;

trait StorageMigrationWriter {
    fn drop_table<K: StorageSerde, V: StorageSerde>(
        &mut self,
        table_id: &TableIdentifier<K, V>,
    ) -> StorageMigrationResult<()>;

    fn get_db_version(&mut self) -> StorageMigrationResult<Option<Version>>;
}

trait StorageMigrationTransaction {
    fn drop_table<K: StorageSerde, V: StorageSerde>(
        self,
        table_id: &TableIdentifier<K, V>,
    ) -> StorageMigrationResult<()>;
}

impl StorageMigrationTransaction for DbWriteTransaction<'_> {
    fn drop_table<K: StorageSerde, V: StorageSerde>(
        self,
        table_id: &TableIdentifier<K, V>,
    ) -> StorageMigrationResult<()> {
        let db = self.open_table(table_id)?.database;
        unsafe {
            self.txn.drop_db(db)?;
        }
        self.commit()?;
        Ok(())
    }
}

impl StorageMigrationWriter for DbWriter {
    fn drop_table<K: StorageSerde, V: StorageSerde>(
        &mut self,
        table_id: &TableIdentifier<K, V>,
    ) -> StorageMigrationResult<()> {
        self.begin_rw_txn()?.drop_table(table_id)?;
        Ok(())
    }

    fn get_db_version(&mut self) -> StorageMigrationResult<Option<Version>> {
        let table_id: TableIdentifier<String, Version> = TableIdentifier::new("storage_version");
        let txn = self.begin_rw_txn()?;
        match txn.open_table(&table_id) {
            Ok(table) => Ok(table.get(&txn, &VERSION_KEY.to_string())?),
            Err(DbError::Inner(libmdbx::Error::NotFound)) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

// Add version to the DB.
#[instrument(skip(db_writer), level = "debug", err)]
fn to_v0(mut db_writer: DbWriter) -> StorageMigrationResult<DbWriter> {
    let version_table_id: TableIdentifier<String, Version> =
        db_writer.create_table("storage_version")?;
    let txn = db_writer.begin_rw_txn()?;
    txn.open_table(&version_table_id)?.insert(&txn, &VERSION_KEY.to_string(), &Version(0))?;
    txn.commit()?;
    Ok(db_writer)
}

#[instrument(skip(db_writer), level = "debug", err)]
pub(crate) fn migrate_db(mut db_writer: DbWriter) -> StorageMigrationResult<DbWriter> {
    debug!("Beginning storage migration.");
    loop {
        let db_version = db_writer.get_db_version()?;
        debug!(version=?db_version, "Upgrading version.");
        db_writer = match db_version {
            // Upgrade storage version.
            None => to_v0(db_writer)?,
            // Storage is up to date.
            Some(STORAGE_VERSION) => {
                info!(%STORAGE_VERSION, "Finished storage migration.");
                return StorageMigrationResult::Ok(db_writer);
            }
            Some(version) if version > STORAGE_VERSION => {
                return Err(StorageMigrationError::UnsupportedDbVersion {
                    crate_version: STORAGE_VERSION,
                    db_version: version,
                });
            }
            _ => unreachable!(),
        };
        debug!(version=?db_version, "Finished upgrading version.");
    }
}
