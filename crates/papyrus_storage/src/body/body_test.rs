use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::transaction::TransactionOffsetInBlock;
use test_case::test_case;
use test_utils::{get_test_block, get_test_body};

use crate::body::events::ThinTransactionOutput;
use crate::body::{BodyStorageReader, BodyStorageWriter, TransactionIndex};
use crate::db::{DbError, KeyAlreadyExistsError};
use crate::test_utils::{get_test_storage, get_test_storage_by_scope};
use crate::{StorageError, StorageScope, StorageWriter};

#[tokio::test]
async fn append_body() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let body = get_test_block(10, None, None, None).body;
    let txs = body.transactions;
    let tx_outputs = body.transaction_outputs;
    let tx_hashes = body.transaction_hashes;

    let body0 = BlockBody {
        transactions: vec![txs[0].clone()],
        transaction_outputs: vec![tx_outputs[0].clone()],
        transaction_hashes: vec![tx_hashes[0]],
    };
    let body1 = BlockBody::default();
    let body2 = BlockBody {
        transactions: vec![txs[1].clone(), txs[2].clone()],
        transaction_outputs: vec![tx_outputs[1].clone(), tx_outputs[2].clone()],
        transaction_hashes: vec![tx_hashes[1], tx_hashes[2]],
    };
    let body3 = BlockBody {
        transactions: vec![txs[3].clone(), txs[0].clone()],
        transaction_outputs: vec![tx_outputs[3].clone(), tx_outputs[0].clone()],
        transaction_hashes: vec![tx_hashes[3], tx_hashes[0]],
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .append_body(BlockNumber(0), body0)
        .unwrap()
        .append_body(BlockNumber(1), body1)
        .unwrap()
        .commit()
        .unwrap();

    // Check for MarkerMismatch error when trying to append the wrong block number.
    let Err(err) = writer.begin_rw_txn().unwrap().append_body(BlockNumber(5), body2.clone()) else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::MarkerMismatch { expected, found }
    if expected == BlockNumber(2) && found == BlockNumber(5));

    writer.begin_rw_txn().unwrap().append_body(BlockNumber(2), body2).unwrap().commit().unwrap();

    let Err(err) = writer.begin_rw_txn().unwrap().append_body(BlockNumber(3), body3) else {
        panic!("Unexpected Ok.");
    };

    let expected_tx_index = TransactionIndex(BlockNumber(3), TransactionOffsetInBlock(1));
    assert_matches!(
        err,
        StorageError::InnerError(DbError::KeyAlreadyExists(KeyAlreadyExistsError {
            table_name: _,
            key,
            value
        })) if key == format!("{:?}", tx_hashes[0]) && value == format!("{:?}", expected_tx_index)
    );

    let txn = reader.begin_ro_txn().unwrap();
    // Check marker.
    assert_eq!(txn.get_body_marker().unwrap(), BlockNumber(3));

    // Check single transactions, outputs and events.
    let tx_cases = vec![
        (BlockNumber(0), TransactionOffsetInBlock(0), Some(0)),
        (BlockNumber(0), TransactionOffsetInBlock(1), None),
        (BlockNumber(1), TransactionOffsetInBlock(0), None),
        (BlockNumber(2), TransactionOffsetInBlock(0), Some(1)),
        (BlockNumber(2), TransactionOffsetInBlock(1), Some(2)),
        (BlockNumber(2), TransactionOffsetInBlock(2), None),
    ];

    for (block_number, tx_offset, original_index) in tx_cases {
        let expected_tx = original_index.map(|i| txs[i].clone());
        assert_eq!(
            txn.get_transaction(TransactionIndex(block_number, tx_offset)).unwrap(),
            expected_tx
        );

        let expected_tx_output =
            original_index.map(|i| ThinTransactionOutput::from(tx_outputs[i].clone()));
        assert_eq!(
            txn.get_transaction_output(TransactionIndex(block_number, tx_offset)).unwrap(),
            expected_tx_output
        );

        let expected_events = original_index.map(|i| tx_outputs[i].events().to_owned());
        assert_eq!(
            txn.get_transaction_events(TransactionIndex(block_number, tx_offset)).unwrap(),
            expected_events
        )
    }

    // Check transaction index by hash.
    assert_eq!(
        txn.get_transaction_idx_by_hash(&tx_hashes[0]).unwrap(),
        Some(TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&tx_hashes[1]).unwrap(),
        Some(TransactionIndex(BlockNumber(2), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&tx_hashes[2]).unwrap(),
        Some(TransactionIndex(BlockNumber(2), TransactionOffsetInBlock(1)))
    );

    // Check transaction hash by index.
    assert_eq!(
        txn.get_transaction_hash_by_idx(&TransactionIndex(
            BlockNumber(0),
            TransactionOffsetInBlock(0)
        ))
        .unwrap(),
        Some(tx_hashes[0])
    );
    assert_eq!(
        txn.get_transaction_hash_by_idx(&TransactionIndex(
            BlockNumber(2),
            TransactionOffsetInBlock(0)
        ))
        .unwrap(),
        Some(tx_hashes[1])
    );
    assert_eq!(
        txn.get_transaction_hash_by_idx(&TransactionIndex(
            BlockNumber(2),
            TransactionOffsetInBlock(1)
        ))
        .unwrap(),
        Some(tx_hashes[2])
    );

    // Check block transactions.
    assert_eq!(txn.get_block_transactions(BlockNumber(0)).unwrap(), Some(vec![txs[0].clone()]));
    assert_eq!(txn.get_block_transactions(BlockNumber(1)).unwrap(), Some(vec![]));
    assert_eq!(
        txn.get_block_transactions(BlockNumber(2)).unwrap(),
        Some(vec![txs[1].clone(), txs[2].clone(),])
    );
    assert_eq!(txn.get_block_transactions(BlockNumber(3)).unwrap(), None);

    // Check block transaction hashes.
    assert_eq!(txn.get_block_transaction_hashes(BlockNumber(0)).unwrap(), Some(vec![tx_hashes[0]]));
    assert_eq!(txn.get_block_transaction_hashes(BlockNumber(1)).unwrap(), Some(vec![]));
    assert_eq!(
        txn.get_block_transaction_hashes(BlockNumber(2)).unwrap(),
        Some(vec![tx_hashes[1], tx_hashes[2]])
    );
    assert_eq!(txn.get_block_transaction_hashes(BlockNumber(3)).unwrap(), None);

    // Check block transaction outputs.
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber(0)).unwrap(),
        Some(vec![ThinTransactionOutput::from(tx_outputs[0].clone())])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber(1)).unwrap(), Some(vec![]));
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber(2)).unwrap(),
        Some(vec![
            ThinTransactionOutput::from(tx_outputs[1].clone()),
            ThinTransactionOutput::from(tx_outputs[2].clone()),
        ])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber(3)).unwrap(), None);
}

#[tokio::test]
async fn append_body_state_only() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_by_scope(StorageScope::StateOnly);
    let block_body = get_test_block(1, Some(1), None, None).body;

    writer
        .begin_rw_txn()
        .unwrap()
        .append_body(BlockNumber(0), block_body)
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    // Check marker.
    assert_eq!(txn.get_body_marker().unwrap(), BlockNumber(1));
}

#[test_case(StorageScope::FullArchive; "revert non existing body fails full archive")]
#[test_case(StorageScope::StateOnly; "revert non existing body fails state only")]
#[tokio::test]
async fn revert_non_existing_body_fails(storage_scope: StorageScope) {
    let ((_, mut writer), _temp_dir) = get_test_storage_by_scope(storage_scope);
    let (_, deleted_data) = writer.begin_rw_txn().unwrap().revert_body(BlockNumber(5)).unwrap();
    assert!(deleted_data.is_none());
}

#[test_case(StorageScope::FullArchive; "revert last body success full archive")]
#[test_case(StorageScope::StateOnly; "revert last body success state only")]
#[tokio::test]
async fn revert_body_state_only(storage_scope: StorageScope) {
    let ((_, mut writer), _temp_dir) = get_test_storage_by_scope(storage_scope);
    writer
        .begin_rw_txn()
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .commit()
        .unwrap();
    writer.begin_rw_txn().unwrap().revert_body(BlockNumber(0)).unwrap().0.commit().unwrap();
}

#[tokio::test]
async fn revert_old_body_fails() {
    let ((_, mut writer), _temp_dir) = get_test_storage();
    append_2_bodies(&mut writer);
    let (_, deleted_data) = writer.begin_rw_txn().unwrap().revert_body(BlockNumber(0)).unwrap();
    assert!(deleted_data.is_none());
}

#[test_case(StorageScope::FullArchive; "revert body updates marker full archive")]
#[test_case(StorageScope::StateOnly; "revert body updates marker state only")]
#[tokio::test]
async fn revert_body_updates_marker(storage_scope: StorageScope) {
    let ((reader, mut writer), _temp_dir) = get_test_storage_by_scope(storage_scope);
    append_2_bodies(&mut writer);

    // Verify that the body marker before revert is 2.
    assert_eq!(reader.begin_ro_txn().unwrap().get_body_marker().unwrap(), BlockNumber(2));

    writer.begin_rw_txn().unwrap().revert_body(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_body_marker().unwrap(), BlockNumber(1));
}

#[tokio::test]
async fn get_reverted_body_returns_none() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    append_2_bodies(&mut writer);

    // Verify that we can get block 1's transactions before the revert.
    assert!(
        reader.begin_ro_txn().unwrap().get_block_transactions(BlockNumber(1)).unwrap().is_some()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_hashes(BlockNumber(1))
            .unwrap()
            .is_some()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_outputs(BlockNumber(1))
            .unwrap()
            .is_some()
    );

    writer.begin_rw_txn().unwrap().revert_body(BlockNumber(1)).unwrap().0.commit().unwrap();
    assert!(
        reader.begin_ro_txn().unwrap().get_block_transactions(BlockNumber(1)).unwrap().is_none()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_hashes(BlockNumber(1))
            .unwrap()
            .is_none()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_outputs(BlockNumber(1))
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn revert_transactions() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let body = get_test_body(10, None, None, None);
    writer
        .begin_rw_txn()
        .unwrap()
        .append_body(BlockNumber(0), body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify the data exists before revert.
    for (offset, tx_hash) in body.transaction_hashes.clone().into_iter().enumerate() {
        let tx_index = TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(offset));

        assert!(reader.begin_ro_txn().unwrap().get_transaction(tx_index).unwrap().is_some());
        assert!(reader.begin_ro_txn().unwrap().get_transaction_output(tx_index).unwrap().is_some());
        assert_eq!(
            reader.begin_ro_txn().unwrap().get_transaction_idx_by_hash(&tx_hash).unwrap().unwrap(),
            tx_index
        );
        assert_eq!(
            reader.begin_ro_txn().unwrap().get_transaction_hash_by_idx(&tx_index).unwrap().unwrap(),
            tx_hash
        );
    }
    assert!(
        reader.begin_ro_txn().unwrap().get_block_transactions(BlockNumber(0)).unwrap().is_some()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_hashes(BlockNumber(0))
            .unwrap()
            .is_some()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_outputs(BlockNumber(0))
            .unwrap()
            .is_some()
    );

    writer.begin_rw_txn().unwrap().revert_body(BlockNumber(0)).unwrap().0.commit().unwrap();

    // Check that all the transactions were deleted.
    for (offset, tx_hash) in body.transaction_hashes.into_iter().enumerate() {
        let tx_index = TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(offset));

        assert!(reader.begin_ro_txn().unwrap().get_transaction(tx_index).unwrap().is_none());
        assert!(reader.begin_ro_txn().unwrap().get_transaction_output(tx_index).unwrap().is_none());
        assert!(reader.begin_ro_txn().unwrap().get_transaction_events(tx_index).unwrap().is_none());
        assert!(
            reader.begin_ro_txn().unwrap().get_transaction_idx_by_hash(&tx_hash).unwrap().is_none()
        );
        assert!(
            reader
                .begin_ro_txn()
                .unwrap()
                .get_transaction_hash_by_idx(&tx_index)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        reader.begin_ro_txn().unwrap().get_block_transactions(BlockNumber(0)).unwrap().is_none()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_hashes(BlockNumber(0))
            .unwrap()
            .is_none()
    );
    assert!(
        reader
            .begin_ro_txn()
            .unwrap()
            .get_block_transaction_outputs(BlockNumber(0))
            .unwrap()
            .is_none()
    );
}

fn append_2_bodies(writer: &mut StorageWriter) {
    writer
        .begin_rw_txn()
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .commit()
        .unwrap();
}
