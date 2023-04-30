use tracing_test::traced_test;

use crate::migration::StorageMigrationWriter;
use crate::test_utils::get_test_config;
use crate::version::VersionStorageReader;
use crate::{open_storage, STORAGE_VERSION};

#[traced_test]
#[test]
fn v0_upgrade() {
    let mut storage_config = get_test_config();
    {
        let (_, mut writer) = open_storage(storage_config.clone()).unwrap();
        let version_table = &writer.tables.storage_version;

        writer.db_writer.drop_table(version_table).unwrap();
    }

    storage_config.migrate_if_necessary = true;
    // Calls migrate_db that will call to_v0 since we dropped the storage_version table.
    let (reader, _) = open_storage(storage_config).unwrap();
    let db_version = reader.begin_ro_txn().unwrap().get_version().unwrap();
    assert_eq!(db_version, Some(STORAGE_VERSION));
}
