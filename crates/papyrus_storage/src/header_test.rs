use assert_matches::assert_matches;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;

use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::get_test_storage;
use crate::{StorageError, StorageWriter};

#[tokio::test]
async fn append_header() {
    let (reader, mut writer) = get_test_storage();

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    if let Err(err) =
        writer.begin_rw_txn().unwrap().append_header(BlockNumber(5), &BlockHeader::default())
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
    assert_eq!(
        reader.begin_ro_txn().unwrap().get_block_number_by_hash(&BlockHash::default()).unwrap(),
        None
    );

    // Append with the right block number.
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();

    // Check block and marker.
    let txn = reader.begin_ro_txn().unwrap();
    let marker = txn.get_header_marker().unwrap();
    assert_eq!(marker, BlockNumber(1));
    let header = txn.get_block_header(BlockNumber(0)).unwrap();
    assert_eq!(header, Some(BlockHeader::default()));

    // Check block hash.
    assert_eq!(txn.get_block_number_by_hash(&BlockHash::default()).unwrap(), Some(BlockNumber(0)));
}

#[tokio::test]
async fn revert_non_existing_header_fails() {
    let (_, mut writer) = get_test_storage();
    let (_, deleted_data) = writer.begin_rw_txn().unwrap().revert_header(BlockNumber(5)).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_last_header_success() {
    let (_, mut writer) = get_test_storage();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();
    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(0)).unwrap().0.commit().unwrap();
}

#[tokio::test]
async fn revert_old_header_fails() {
    let (_, mut writer) = get_test_storage();
    append_2_headers(&mut writer);
    let (_, deleted_data) = writer.begin_rw_txn().unwrap().revert_header(BlockNumber(0)).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_header_updates_marker() {
    let (reader, mut writer) = get_test_storage();
    append_2_headers(&mut writer);

    // Verify that the header marker before revert is 2.
    assert_eq!(reader.begin_ro_txn().unwrap().get_header_marker().unwrap(), BlockNumber(2));

    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_header_marker().unwrap(), BlockNumber(1));
}

#[tokio::test]
async fn get_reverted_header_returns_none() {
    let (reader, mut writer) = get_test_storage();
    append_2_headers(&mut writer);

    // Verify that we can get block 1's header before the revert.
    assert!(reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(1)).unwrap().is_some());

    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert!(reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(1)).unwrap().is_none());
}

#[tokio::test]
async fn get_reverted_block_number_by_hash_returns_none() {
    let (reader, mut writer) = get_test_storage();
    append_2_headers(&mut writer);

    let block_hash = BlockHash(stark_felt!("0x1"));

    // Verify that we can get block 1 by hash before the revert.
    assert!(
        reader.begin_ro_txn().unwrap().get_block_number_by_hash(&block_hash).unwrap().is_some()
    );

    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert!(
        reader.begin_ro_txn().unwrap().get_block_number_by_hash(&block_hash).unwrap().is_none()
    );
}

fn append_2_headers(writer: &mut StorageWriter) {
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader { block_hash: BlockHash(stark_felt!("0x0")), ..BlockHeader::default() },
        )
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader { block_hash: BlockHash(stark_felt!("0x1")), ..BlockHeader::default() },
        )
        .unwrap()
        .commit()
        .unwrap();
}
