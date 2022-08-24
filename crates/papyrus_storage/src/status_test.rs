use assert_matches::assert_matches;
use starknet_api::{BlockNumber, BlockStatus};

use super::{StatusStorageReader, StatusStorageWriter, StorageError};
use crate::test_utils::get_test_storage;

#[tokio::test]
async fn test_append_status() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    if let Err(err) =
        writer.begin_rw_txn()?.append_block_status(BlockNumber(5), &BlockStatus::default())
    {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected: BlockNumber(0), found: BlockNumber(5) }
        );
    } else {
        panic!("Unexpected Ok.");
    }

    // Append with the right block number.
    writer
        .begin_rw_txn()?
        .append_block_status(BlockNumber(0), &BlockStatus::default())?
        .commit()?;

    // Check block status and marker.
    let txn = reader.begin_ro_txn()?;
    let marker = txn.get_status_marker()?;
    assert_eq!(marker, BlockNumber(1));
    let status = txn.get_block_status(BlockNumber(0))?;
    assert_eq!(status, Some(BlockStatus::default()));

    Ok(())
}
