use assert_matches::assert_matches;
use starknet_api::{
    shash, BlockBody, BlockNumber, CallData, ClassHash, ContractAddress, ContractAddressSalt,
    DeployTransaction, DeployTransactionOutput, Fee, StarkHash, Transaction, TransactionHash,
    TransactionOffsetInBlock, TransactionOutput, TransactionVersion,
};

use super::{BodyStorageReader, BodyStorageWriter, StorageError};
use crate::test_utils::get_test_storage;

#[tokio::test]
async fn test_append_body() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();

    let txs: Vec<Transaction> = (0..10)
        .map(|i| {
            Transaction::Deploy(DeployTransaction::new(
                TransactionHash(StarkHash::from_u64(i as u64)),
                TransactionVersion(shash!("0x1")),
                ClassHash(StarkHash::from_u64(i as u64)),
                ContractAddress(StarkHash::from_u64(i as u64)),
                ContractAddressSalt(shash!("0x2")),
                CallData(vec![StarkHash::from_u64(i as u64)]),
            ))
        })
        .collect();
    let tx_outputs: Vec<TransactionOutput> = (0..10)
        .map(|i| TransactionOutput::Deploy(DeployTransactionOutput::new(Fee(i as u128))))
        .collect();

    let body0 = BlockBody {
        transactions: vec![txs[0].clone()],
        transaction_outputs: vec![tx_outputs[0].clone()],
    };
    let body1 = BlockBody { transactions: vec![], transaction_outputs: vec![] };
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
        .append_body(BlockNumber(0), &body0)?
        .append_body(BlockNumber(1), &body1)?
        .commit()?;

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber(5), &body2) {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected: BlockNumber(2), found: BlockNumber(5) }
        );
    } else {
        panic!("Unexpected Ok.");
    }

    writer.begin_rw_txn()?.append_body(BlockNumber(2), &body2)?.commit()?;

    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber(3), &body3) {
        assert_matches!(
            err,
            StorageError::TransactionHashAlreadyExists {
                tx_hash,
                block_number: BlockNumber(3),
                tx_offset_in_block: TransactionOffsetInBlock(1)
            } if tx_hash == txs[0].transaction_hash()
        );
    } else {
        panic!("Unexpected Ok.");
    }

    let txn = reader.begin_ro_txn()?;
    // Check marker.
    assert_eq!(txn.get_body_marker()?, BlockNumber(3));

    // Check single transactions and outputs.
    let tx_cases = vec![
        (BlockNumber(0), TransactionOffsetInBlock(0), Some(0)),
        (BlockNumber(0), TransactionOffsetInBlock(1), None),
        (BlockNumber(1), TransactionOffsetInBlock(0), None),
        (BlockNumber(2), TransactionOffsetInBlock(0), Some(1)),
        (BlockNumber(2), TransactionOffsetInBlock(1), Some(2)),
        (BlockNumber(2), TransactionOffsetInBlock(2), None),
    ];

    for (block_number, tx_offset, original_index) in tx_cases {
        let expected_tx = original_index.map(|i| &txs[i]);
        assert_eq!(txn.get_transaction(block_number, tx_offset)?.as_ref(), expected_tx);
        let expected_tx_output = original_index.map(|i| &tx_outputs[i]);
        assert_eq!(
            txn.get_transaction_output(block_number, tx_offset)?.as_ref(),
            expected_tx_output
        );
    }

    // Check transaction hash.
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[0].transaction_hash())?,
        Some((BlockNumber(0), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[1].transaction_hash())?,
        Some((BlockNumber(2), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        txn.get_transaction_idx_by_hash(&txs[2].transaction_hash())?,
        Some((BlockNumber(2), TransactionOffsetInBlock(1)))
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
        Some(vec![tx_outputs[0].clone()])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber(1))?, Some(vec![]));
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber(2))?,
        Some(vec![tx_outputs[1].clone(), tx_outputs[2].clone()])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber(3))?, None);
    Ok(())
}
