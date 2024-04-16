use assert_matches::assert_matches;
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::ChainId;

use crate::db::{DbConfig, DbError};
use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::mmap_file::MmapFileConfig;
use crate::test_utils::get_test_config;
use crate::{open_storage, open_storage_ro, StorageConfig, StorageError, StorageScope};

// #[test]
// fn open_ro() {
//     let (config, _db_handle) = get_test_config(None);
//     let (_reader, mut writer) = open_storage(config.clone()).unwrap();
//     let Err(err) = open_storage(config.clone()) else {
//         panic!("Should not be able to open the same storage twice with read-write permissions");
//     };
//     dbg!(&err);
//     println!("{}", err);
//     assert_matches!(err, StorageError::InnerError(DbError::Inner(_)));

//     let another_reader = open_storage_ro(config)
//         .expect("Should be able to open the same storage twice with read-only permissions");

//     let header = another_reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
//     assert!(header.is_none());

//     writer
//         .begin_rw_txn()
//         .unwrap()
//         .append_header(BlockNumber(0), &BlockHeader::default())
//         .unwrap()
//         .commit()
//         .unwrap();

//     let header = another_reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
//     assert!(header.is_some());
// }

#[test]
fn open_another() {
    let config = StorageConfig{
        db_config: DbConfig {
            path_prefix: "/app/data".into(),
            chain_id: ChainId("SN_MAIN".into()),
            enforce_file_exists: false,
            min_size: 1048576,
            max_size: 1099511627776,
            growth_step: 4294967296,
        },
        mmap_file_config: MmapFileConfig {
            max_size: 1099511627776,
            growth_step: 1073741824,
            max_object_size: 1048576,
        },
        scope: StorageScope::FullArchive,
    };

    let reader = open_storage_ro(config).unwrap();

    let header = reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
}
