use tempfile::tempdir;

use crate::starknet::{BlockHeader, BlockNumber};

use super::{open_block_storage, BlockStorageReader, BlockStorageWriter};

fn get_test_storage() -> (BlockStorageReader, BlockStorageWriter) {
    let dir = tempdir().unwrap();
    open_block_storage(dir.path()).expect("Failed to open storage.")
}

#[tokio::test]
async fn test_append_header() {
    let (reader, mut writer) = get_test_storage();
    writer
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap();

    let marker = reader.get_header_marker().unwrap();
    assert_eq!(marker, BlockNumber(1));
    let header = reader.get_block_header(BlockNumber(0)).unwrap();
    assert_eq!(header, Some(BlockHeader::default()));
}
