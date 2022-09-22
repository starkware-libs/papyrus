use assert_matches::assert_matches;
use starknet_api::{BlockBody, BlockNumber, TransactionOffsetInBlock};

use super::events::ThinTransactionOutput;
use super::{BodyStorageReader, BodyStorageWriter};
use crate::test_utils::{get_test_block, get_test_storage};
use crate::{StorageError, TransactionIndex};

#[tokio::test]
async fn append_body() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let body = get_test_block(10).body;
    let txs = body.transactions();
    let tx_outputs = body.transaction_outputs();

    let body0 = BlockBody::new(vec![txs[0].clone()], vec![tx_outputs[0].clone()]).unwrap();
    let body1 = BlockBody::new(vec![], vec![]).unwrap();
    let body2 = BlockBody::new(
        vec![txs[1].clone(), txs[2].clone()],
        vec![tx_outputs[1].clone(), tx_outputs[2].clone()],
    )
    .unwrap();
    let body3 = BlockBody::new(
        vec![txs[3].clone(), txs[0].clone()],
        vec![tx_outputs[3].clone(), tx_outputs[0].clone()],
    )
    .unwrap();
    writer
        .begin_rw_txn()?
        .append_body(BlockNumber::new(0), body0)?
        .append_body(BlockNumber::new(1), body1)?
        .commit()?;

    // Check for MarkerMismatch error when trying to append the wrong block number.
    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber::new(5), body2.clone()) {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected, found }
        if expected == BlockNumber::new(2) && found == BlockNumber::new(5));
    } else {
        panic!("Unexpected Ok.");
    }

    writer.begin_rw_txn()?.append_body(BlockNumber::new(2), body2)?.commit()?;

    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber::new(3), body3) {
        let expected_tx_index = TransactionIndex(BlockNumber::new(3), TransactionOffsetInBlock(1));
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
    assert_eq!(txn.get_body_marker()?, BlockNumber::new(3));

    // Check single transactions, outputs and events.
    let tx_cases = vec![
        (BlockNumber::new(0), TransactionOffsetInBlock(0), Some(0)),
        (BlockNumber::new(0), TransactionOffsetInBlock(1), None),
        (BlockNumber::new(1), TransactionOffsetInBlock(0), None),
        (BlockNumber::new(2), TransactionOffsetInBlock(0), Some(1)),
        (BlockNumber::new(2), TransactionOffsetInBlock(1), Some(2)),
        (BlockNumber::new(2), TransactionOffsetInBlock(2), None),
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

        let expected_events = original_index.map(|i| tx_outputs[i].events().clone());
        assert_eq!(
            txn.get_transaction_events(TransactionIndex(block_number, tx_offset))?,
            expected_events
        )
    }

    // Check transaction hash.
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[0].transaction_hash())?,
        Some(TransactionIndex(BlockNumber::new(0), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[1].transaction_hash())?,
        Some(TransactionIndex(BlockNumber::new(2), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[2].transaction_hash())?,
        Some(TransactionIndex(BlockNumber::new(2), TransactionOffsetInBlock(1)))
    );

    // Check block transactions.
    assert_eq!(txn.get_block_transactions(BlockNumber::new(0))?, Some(vec![txs[0].clone()]));
    assert_eq!(txn.get_block_transactions(BlockNumber::new(1))?, Some(vec![]));
    assert_eq!(
        txn.get_block_transactions(BlockNumber::new(2))?,
        Some(vec![txs[1].clone(), txs[2].clone()])
    );
    assert_eq!(txn.get_block_transactions(BlockNumber::new(3))?, None);

    // Check block transaction outputs.
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber::new(0))?,
        Some(vec![ThinTransactionOutput::from(tx_outputs[0].clone())])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber::new(1))?, Some(vec![]));
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber::new(2))?,
        Some(vec![
            ThinTransactionOutput::from(tx_outputs[1].clone()),
            ThinTransactionOutput::from(tx_outputs[2].clone()),
        ])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber::new(3))?, None);
    Ok(())
}
