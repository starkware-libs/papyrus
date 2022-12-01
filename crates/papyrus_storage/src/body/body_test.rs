use assert_matches::assert_matches;
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::transaction::TransactionOffsetInBlock;

use crate::body::events::ThinTransactionOutput;
use crate::body::{BodyStorageReader, BodyStorageWriter};
use crate::test_utils::{get_test_block, get_test_body, get_test_storage};
use crate::{StorageError, StorageWriter, TransactionIndex};

#[tokio::test]
async fn append_body() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let body = get_test_block(10).body;
    let txs = body.transactions;
    let tx_outputs = body.transaction_outputs;

    let body0 = BlockBody {
        transactions: vec![txs[0].clone()],
        transaction_outputs: vec![tx_outputs[0].clone()],
    };
    let body1 = BlockBody::default();
    let body2 = BlockBody {
        transactions: vec![txs[1].clone(), txs[2].clone()],
        transaction_outputs: vec![tx_outputs[1].clone(), tx_outputs[2].clone()],
    };
    let body3 = BlockBody {
        transactions: vec![txs[3].clone(), txs[0].clone()],
        transaction_outputs: vec![tx_outputs[3].clone(), tx_outputs[0].clone()],
    };
    writer
        .begin_rw_txn()?
        .append_body(BlockNumber(0), body0)?
        .append_body(BlockNumber(1), body1)?
        .commit()?;

    // Check for MarkerMismatch error when trying to append the wrong block number.
    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber(5), body2.clone()) {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected, found }
        if expected == BlockNumber(2) && found == BlockNumber(5));
    } else {
        panic!("Unexpected Ok.");
    }

    writer.begin_rw_txn()?.append_body(BlockNumber(2), body2)?.commit()?;

    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber(3), body3) {
        let expected_tx_index = TransactionIndex(BlockNumber(3), TransactionOffsetInBlock(1));
        assert_matches!(
            err,
            StorageError::TransactionHashAlreadyExists {
                tx_hash,
                transaction_index
            } if tx_hash == txs[0].transaction_hash() && transaction_index == expected_tx_index
        );
    } else {
        panic!("Unexpected Ok.");
    }

    let txn = reader.begin_ro_txn()?;
    // Check marker.
    assert_eq!(txn.get_body_marker()?, BlockNumber(3));

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
        assert_eq!(txn.get_transaction(TransactionIndex(block_number, tx_offset))?, expected_tx);

        let expected_tx_output =
            original_index.map(|i| ThinTransactionOutput::from(tx_outputs[i].clone()));
        assert_eq!(
            txn.get_transaction_output(TransactionIndex(block_number, tx_offset))?,
            expected_tx_output
        );

        let expected_events = original_index.map(|i| tx_outputs[i].events().to_owned());
        assert_eq!(
            txn.get_transaction_events(TransactionIndex(block_number, tx_offset))?,
            expected_events
        )
    }

    // Check transaction hash.
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[0].transaction_hash())?,
        Some(TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[1].transaction_hash())?,
        Some(TransactionIndex(BlockNumber(2), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[2].transaction_hash())?,
        Some(TransactionIndex(BlockNumber(2), TransactionOffsetInBlock(1)))
    );

    // Check block transactions.
    assert_eq!(txn.get_block_transactions(BlockNumber(0))?, Some(vec![txs[0].clone()]));
    assert_eq!(txn.get_block_transactions(BlockNumber(1))?, Some(vec![]));
    assert_eq!(
        txn.get_block_transactions(BlockNumber(2))?,
        Some(vec![txs[1].clone(), txs[2].clone()])
    );
    assert_eq!(txn.get_block_transactions(BlockNumber(3))?, None);

    // Check block transaction outputs.
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber(0))?,
        Some(vec![ThinTransactionOutput::from(tx_outputs[0].clone())])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber(1))?, Some(vec![]));
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber(2))?,
        Some(vec![
            ThinTransactionOutput::from(tx_outputs[1].clone()),
            ThinTransactionOutput::from(tx_outputs[2].clone()),
        ])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber(3))?, None);
    Ok(())
}

#[tokio::test]
async fn revert_non_existing_body_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    if let Err(err) = writer.begin_rw_txn()?.revert_body(BlockNumber(5)) {
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
async fn revert_last_body_success() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    writer.begin_rw_txn()?.append_body(BlockNumber(0), BlockBody::default())?.commit()?;
    writer.begin_rw_txn()?.revert_body(BlockNumber(0))?.commit()?;
    Ok(())
}

#[tokio::test]
async fn revert_old_body_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    append_2_bodies(&mut writer)?;
    if let Err(err) = writer.begin_rw_txn()?.revert_body(BlockNumber(0)) {
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
async fn revert_body_updates_marker() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_bodies(&mut writer)?;

    // Verify that the body marker before revert is 2.
    assert_eq!(reader.begin_ro_txn()?.get_body_marker()?, BlockNumber(2));

    writer.begin_rw_txn()?.revert_body(BlockNumber(1))?.commit()?;
    assert_eq!(reader.begin_ro_txn()?.get_body_marker()?, BlockNumber(1));

    Ok(())
}

#[tokio::test]
async fn get_reverted_body_returns_none() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_bodies(&mut writer)?;

    // Verify that we can get block 1's transactions before the revert.
    assert!(reader.begin_ro_txn()?.get_block_transactions(BlockNumber(1))?.is_some());
    assert!(reader.begin_ro_txn()?.get_block_transaction_outputs(BlockNumber(1))?.is_some());

    writer.begin_rw_txn()?.revert_body(BlockNumber(1))?.commit()?;
    assert!(reader.begin_ro_txn()?.get_block_transactions(BlockNumber(1))?.is_none());
    assert!(reader.begin_ro_txn()?.get_block_transaction_outputs(BlockNumber(1))?.is_none());

    Ok(())
}

#[tokio::test]
async fn revert_transactions() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let body = get_test_body(10);
    writer.begin_rw_txn()?.append_body(BlockNumber(0), body.clone())?.commit()?;

    for (offset, tx_hash) in body.transactions.iter().map(|tx| tx.transaction_hash()).enumerate() {
        let tx_index = TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(offset));

        assert!(reader.begin_ro_txn()?.get_transaction(tx_index)?.is_some());
        assert!(reader.begin_ro_txn()?.get_transaction_output(tx_index)?.is_some());
        assert_eq!(
            reader.begin_ro_txn()?.get_transaction_idx_by_hash(&tx_hash)?.unwrap(),
            tx_index
        );
    }

    writer.begin_rw_txn()?.revert_body(BlockNumber(0))?.commit()?;

    for (offset, tx_hash) in body.transactions.iter().map(|tx| tx.transaction_hash()).enumerate() {
        let tx_index = TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(offset));

        assert!(reader.begin_ro_txn()?.get_transaction(tx_index)?.is_none());
        assert!(reader.begin_ro_txn()?.get_transaction_output(tx_index)?.is_none());
        assert!(reader.begin_ro_txn()?.get_transaction_idx_by_hash(&tx_hash)?.is_none());
    }

    Ok(())
}

fn append_2_bodies(writer: &mut StorageWriter) -> Result<(), anyhow::Error> {
    writer
        .begin_rw_txn()?
        .append_body(BlockNumber(0), BlockBody::default())?
        .append_body(BlockNumber(1), BlockBody::default())?
        .commit()?;

    Ok(())
}
