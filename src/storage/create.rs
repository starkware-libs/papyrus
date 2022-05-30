use std::path::Path;

use super::{
    components::{info_prepare, InfoReader, InfoWriter, StorageComponents},
    db::{open_env, StorageError},
};

/**
 * This is the function that's supposed to be called by the function that initializes
 * the store and wires it to relevant other modules.
 */
pub async fn create_storage_components(path: &Path) -> Result<StorageComponents, StorageError> {
    let (db_reader, db_writer_mutex) = open_env(path)?;

    // Create databases if needed.
    {
        let mut db_writer = db_writer_mutex.lock().await;
        info_prepare(&mut db_writer)?;
    }

    let info_reader = InfoReader::new(db_reader);
    let info_writer = InfoWriter::new(db_writer_mutex);

    Ok(StorageComponents {
        info_reader,
        info_writer,
    })
}
