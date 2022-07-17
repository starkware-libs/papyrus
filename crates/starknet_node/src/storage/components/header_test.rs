use starknet_api::{BlockHash, BlockHeader, BlockNumber};

use super::*;
use crate::storage::test_utils::get_test_storage;

#[tokio::test]
async fn test_append_header() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    if let Err(err) = writer.begin_rw_txn()?.append_header(BlockNumber(5), &BlockHeader::default())
    {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected: BlockNumber(0), found: BlockNumber(5) }
        );
    } else {
        panic!("Unexpected Ok.");
    }
    // Check block hash.
    assert_eq!(reader.begin_ro_txn()?.get_block_number_by_hash(&BlockHash::default())?, None);

    // Append with the right block number.
    writer.begin_rw_txn()?.append_header(BlockNumber(0), &BlockHeader::default())?.commit()?;

    // Check block and marker.
    let txn = reader.begin_ro_txn()?;
    let marker = txn.get_header_marker()?;
    assert_eq!(marker, BlockNumber(1));
    let header = txn.get_block_header(BlockNumber(0))?;
    assert_eq!(header, Some(BlockHeader::default()));

    // Check block hash.
    assert_eq!(txn.get_block_number_by_hash(&BlockHash::default())?, Some(BlockNumber(0)));

    Ok(())
}
