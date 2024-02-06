use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_types_core::felt::Felt;

use crate::header::{HeaderStorageReader, HeaderStorageWriter, StarknetVersion};
use crate::test_utils::get_test_storage;
use crate::{StorageError, StorageWriter};

#[tokio::test]
async fn append_header() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // Check for MarkerMismatch error when trying to append the wrong block number.
    let Err(err) =
        writer.begin_rw_txn().unwrap().append_header(BlockNumber(5), &BlockHeader::default())
    else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::MarkerMismatch { expected, found }
        if expected == BlockNumber(0) && found == BlockNumber(5)
    );

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
    let ((_, mut writer), _temp_dir) = get_test_storage();
    let (_, deleted_data, _) =
        writer.begin_rw_txn().unwrap().revert_header(BlockNumber(5)).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_last_header_success() {
    let ((_, mut writer), _temp_dir) = get_test_storage();
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
    let ((_, mut writer), _temp_dir) = get_test_storage();
    append_2_headers(&mut writer);
    let (_, deleted_data, _) =
        writer.begin_rw_txn().unwrap().revert_header(BlockNumber(0)).unwrap();
    assert!(deleted_data.is_none());
}

#[tokio::test]
async fn revert_header_updates_marker() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    append_2_headers(&mut writer);

    // Verify that the header marker before revert is 2.
    assert_eq!(reader.begin_ro_txn().unwrap().get_header_marker().unwrap(), BlockNumber(2));

    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_header_marker().unwrap(), BlockNumber(1));
}

#[tokio::test]
async fn get_reverted_header_returns_none() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    append_2_headers(&mut writer);

    // Verify that we can get block 1's header before the revert.
    assert!(reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(1)).unwrap().is_some());

    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert!(reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(1)).unwrap().is_none());
}

#[tokio::test]
async fn get_reverted_block_number_by_hash_returns_none() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    append_2_headers(&mut writer);

    let block_hash = BlockHash(Felt::ONE);

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
            &BlockHeader { block_hash: BlockHash(Felt::ZERO), ..BlockHeader::default() },
        )
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader { block_hash: BlockHash(Felt::ONE), ..BlockHeader::default() },
        )
        .unwrap()
        .commit()
        .unwrap();
}

#[tokio::test]
async fn starknet_version() {
    fn block_header(hash: u8) -> BlockHeader {
        BlockHeader { block_hash: BlockHash(Felt::from(hash)), ..BlockHeader::default() }
    }

    let ((reader, mut writer), _temp_dir) = get_test_storage();

    let initial_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(0)).unwrap();
    assert!(initial_starknet_version.is_none());

    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &block_header(0))
        .unwrap()
        .update_starknet_version(&BlockNumber(0), &StarknetVersion::default())
        .unwrap()
        .commit()
        .unwrap();

    let block_0_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(0)).unwrap();
    assert_eq!(block_0_starknet_version.unwrap(), StarknetVersion::default());

    let non_existing_block_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(1)).unwrap();
    assert!(non_existing_block_starknet_version.is_none());

    let second_version = StarknetVersion("second_version".to_string());
    let yet_another_version = StarknetVersion("yet_another_version".to_string());

    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(1), &block_header(1))
        .unwrap()
        .update_starknet_version(&BlockNumber(1), &StarknetVersion::default())
        .unwrap()
        .append_header(BlockNumber(2), &block_header(2))
        .unwrap()
        .update_starknet_version(&BlockNumber(2), &second_version)
        .unwrap()
        .append_header(BlockNumber(3), &block_header(3))
        .unwrap()
        .update_starknet_version(&BlockNumber(3), &second_version)
        .unwrap()
        .append_header(BlockNumber(4), &block_header(4))
        .unwrap()
        .update_starknet_version(&BlockNumber(4), &yet_another_version)
        .unwrap()
        .append_header(BlockNumber(5), &block_header(5))
        .unwrap()
        .update_starknet_version(&BlockNumber(5), &yet_another_version)
        .unwrap()
        .commit()
        .unwrap();

    let block_0_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(0)).unwrap();
    assert_eq!(block_0_starknet_version.unwrap(), StarknetVersion::default());

    let block_1_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(1)).unwrap();
    assert_eq!(block_1_starknet_version.unwrap(), StarknetVersion::default());

    let block_2_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(2)).unwrap();
    assert_eq!(block_2_starknet_version.unwrap(), second_version);

    let block_3_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(3)).unwrap();
    assert_eq!(block_3_starknet_version.unwrap(), second_version);

    let block_4_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(4)).unwrap();
    assert_eq!(block_4_starknet_version.unwrap(), yet_another_version);

    let block_5_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(5)).unwrap();
    assert_eq!(block_5_starknet_version.unwrap(), yet_another_version);

    let block_6_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(6)).unwrap();
    assert!(block_6_starknet_version.is_none());

    // Revert block 5.
    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(5)).unwrap().0.commit().unwrap();
    let block_5_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(5)).unwrap();
    assert!(block_5_starknet_version.is_none());

    let block_4_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(4)).unwrap();
    assert_eq!(block_4_starknet_version.unwrap(), yet_another_version);

    // Revert block 4.
    writer.begin_rw_txn().unwrap().revert_header(BlockNumber(4)).unwrap().0.commit().unwrap();
    let block_4_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(4)).unwrap();
    assert!(block_4_starknet_version.is_none());

    let block_3_starknet_version =
        reader.begin_ro_txn().unwrap().get_starknet_version(BlockNumber(3)).unwrap();
    assert_eq!(block_3_starknet_version.unwrap(), second_version);
}

#[test]
fn block_signature() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    assert!(reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap().is_none());
    let signature = BlockSignature::default();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_block_signature(BlockNumber(0), &signature)
        .unwrap()
        .commit()
        .unwrap();
    assert_eq!(
        reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap(),
        Some(signature)
    );
}

#[test]
fn get_reverted_block_signature_returns_none() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let signature = BlockSignature::default();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_block_signature(BlockNumber(0), &signature)
        .unwrap()
        .commit()
        .unwrap();
    assert_eq!(
        reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap(),
        Some(signature)
    );
    let (txn, maybe_header, maybe_signature) =
        writer.begin_rw_txn().unwrap().revert_header(BlockNumber(0)).unwrap();
    txn.commit().unwrap();
    assert!(maybe_header.is_some());
    assert!(maybe_signature.is_some());
    assert!(reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap().is_none());
}
