use crate::starknet::{BlockHash, BlockHeader, BlockNumber};
use crate::storage::components::block::test_utils::get_test_storage;
use crate::storage::components::{HeaderStorageReader, HeaderStorageWriter};

use super::BlockStorageError;

#[tokio::test]
async fn test_append_header() {
    let (reader, mut writer) = get_test_storage();

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    assert_matches!(
        writer
            .append_header(BlockNumber(5), &BlockHeader::default())
            .unwrap_err(),
        BlockStorageError::MarkerMismatch {
            expected: BlockNumber(0),
            found: BlockNumber(5)
        }
    );

    // Check block hash.
    assert_eq!(
        reader
            .get_block_number_by_hash(&BlockHash::default())
            .unwrap(),
        None
    );

    // Append with the right block number.
    writer
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap();

    // Check block and marker.
    let marker = reader.get_header_marker().unwrap();
    assert_eq!(marker, BlockNumber(1));
    let header = reader.get_block_header(BlockNumber(0)).unwrap();
    assert_eq!(header, Some(BlockHeader::default()));

    // Check block hash.
    assert_eq!(
        reader
            .get_block_number_by_hash(&BlockHash::default())
            .unwrap(),
        Some(BlockNumber(0))
    );
}
