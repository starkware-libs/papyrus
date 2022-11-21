use assert_matches::assert_matches;
use starknet_api::{shash, BlockHash, BlockHeader, BlockNumber, StarkHash};

use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::get_test_storage;
use crate::{StorageError, StorageWriter};

#[tokio::test]
async fn append_header() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    if let Err(err) = writer.begin_rw_txn()?.append_header(BlockNumber(5), &BlockHeader::default())
    {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected, found }
            if expected == BlockNumber(0) && found == BlockNumber(5)
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

#[tokio::test]
async fn revert_non_existing_header_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    if let Err(err) = writer.begin_rw_txn()?.revert_header(BlockNumber(5)) {
        assert_matches!(
            err,
            StorageError::InvalidRevert {
                revert_block_number,
                block_number_marker
            }
            if revert_block_number == BlockNumber(5) && block_number_marker == BlockNumber(0)
        )
    } else {
        panic!("Unexpected Ok.");
    }
    Ok(())
}

#[tokio::test]
async fn revert_last_header_success() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    writer.begin_rw_txn()?.append_header(BlockNumber(0), &BlockHeader::default())?.commit()?;
    writer.begin_rw_txn()?.revert_header(BlockNumber(0))?.commit()?;
    Ok(())
}

#[tokio::test]
async fn revert_old_header_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    append_2_headers(&mut writer)?;
    if let Err(err) = writer.begin_rw_txn()?.revert_header(BlockNumber(0)) {
        assert_matches!(
            err,
            StorageError::InvalidRevert {
                revert_block_number,
                block_number_marker
            }
            if revert_block_number == BlockNumber(0) && block_number_marker == BlockNumber(2)
        );
    } else {
        panic!("Unexpected Ok.");
    }
    Ok(())
}

#[tokio::test]
async fn revert_header_updates_marker() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_headers(&mut writer)?;

    // Verify that the header marker before revert is 2.
    assert_eq!(reader.begin_ro_txn()?.get_header_marker()?, BlockNumber(2));

    writer.begin_rw_txn()?.revert_header(BlockNumber(1))?.commit()?;
    assert_eq!(reader.begin_ro_txn()?.get_header_marker()?, BlockNumber(1));

    Ok(())
}

#[tokio::test]
async fn get_reverted_header_returns_none() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_headers(&mut writer)?;

    // Verify that we can get block 1's header before the revert.
    assert!(reader.begin_ro_txn()?.get_block_header(BlockNumber(1))?.is_some());

    writer.begin_rw_txn()?.revert_header(BlockNumber(1))?.commit()?;
    assert!(reader.begin_ro_txn()?.get_block_header(BlockNumber(1))?.is_none());

    Ok(())
}

#[tokio::test]
async fn get_reverted_block_number_by_hash_returns_none() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_headers(&mut writer)?;

    let block_hash = BlockHash(shash!("0x1"));

    // Verify that we can get block 1 by hash before the revert.
    assert!(reader.begin_ro_txn()?.get_block_number_by_hash(&block_hash)?.is_some());

    writer.begin_rw_txn()?.revert_header(BlockNumber(1))?.commit()?;
    assert!(reader.begin_ro_txn()?.get_block_number_by_hash(&block_hash)?.is_none());

    Ok(())
}

fn append_2_headers(writer: &mut StorageWriter) -> Result<(), anyhow::Error> {
    writer
        .begin_rw_txn()?
        .append_header(
            BlockNumber(0),
            &BlockHeader { block_hash: BlockHash(shash!("0x0")), ..BlockHeader::default() },
        )?
        .append_header(
            BlockNumber(1),
            &BlockHeader { block_hash: BlockHash(shash!("0x1")), ..BlockHeader::default() },
        )?
        .commit()?;

    Ok(())
}
