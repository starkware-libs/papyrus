use tempfile::tempdir;

use crate::starknet::BlockNumber;

use super::{open_block_storage, BlockStorageReader, BlockStorageWriter};

fn get_test_storage() -> (BlockStorageReader, BlockStorageWriter) {
    let dir = tempdir().unwrap();
    open_block_storage(dir.path()).expect("Failed to open storage.")
}

#[tokio::test]
async fn test_add_block_number() {
    let (reader, mut writer) = get_test_storage();
    let expected = BlockNumber(5);

    assert_eq!(
        reader.get_block_number_marker().unwrap(),
        BlockNumber::default()
    );
    writer.set_block_number_marker(expected).unwrap();
    assert_eq!(reader.get_block_number_marker().unwrap(), expected);
}
