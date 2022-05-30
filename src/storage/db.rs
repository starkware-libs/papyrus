use std::{path::Path, sync::Arc};

use libmdbx::WriteMap;
use tokio::sync::Mutex;

// Maximum number of Sub-Databases.
const MAX_DBS: usize = 10;

// Note that NO_TLS mode is used by default.
type EnvironmentKind = WriteMap;
type Environment = libmdbx::Environment<EnvironmentKind>;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("Synchronization error")]
    AccessSyncError {},
    #[error("Database error")]
    DatabaseError(#[from] libmdbx::Error),
}

/// Opens an MDBX environment and returns a reader and a writer to it.
/// The writer is wrapped in a mutex to make sure there is only one write transaction at any given
/// moment.
pub fn open_env(path: &Path) -> Result<(DbReader, Arc<Mutex<DbWriter>>), libmdbx::Error> {
    let env = Arc::new(Environment::new().set_max_dbs(MAX_DBS).open(path)?);
    Ok((
        DbReader { env: env.clone() },
        Arc::new(Mutex::new(DbWriter { env })),
    ))
}

#[derive(Clone)]
pub struct DbReader {
    env: Arc<Environment>,
}

pub struct DbWriter {
    env: Arc<Environment>,
}

impl DbReader {
    pub fn begin_ro_txn(
        &self,
    ) -> libmdbx::Result<libmdbx::Transaction<'_, libmdbx::RO, EnvironmentKind>> {
        self.env.begin_ro_txn()
    }
}

impl DbWriter {
    pub fn begin_rw_txn(
        &self,
    ) -> libmdbx::Result<libmdbx::Transaction<'_, libmdbx::RW, EnvironmentKind>> {
        self.env.begin_rw_txn()
    }
}
