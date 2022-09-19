use assert_matches::assert_matches;
use starknet_api::{
    shash, BlockBody, BlockNumber, CallData, ClassHash, ContractAddress, ContractAddressSalt,
    DeployTransaction, DeployTransactionOutput, Fee, StarkHash, Transaction, TransactionHash,
    TransactionOffsetInBlock, TransactionOutput, TransactionVersion,
};

use super::{BodyStorageReader, BodyStorageWriter, StorageError};
use crate::test_utils::{get_test_body, get_test_storage};
use crate::{StorageWriter, TransactionIndex};

#[tokio::test]
async fn append_body() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();

    let txs: Vec<Transaction> = (0..10)
        .map(|i| {
            Transaction::Deploy(DeployTransaction {
                transaction_hash: TransactionHash(StarkHash::from_u64(i as u64)),
                version: TransactionVersion(shash!("0x1")),
                contract_address: ContractAddress::try_from(StarkHash::from_u64(i as u64)).unwrap(),
                constructor_calldata: CallData(vec![StarkHash::from_u64(i as u64)]),
                class_hash: ClassHash::new(StarkHash::from_u64(i as u64)),
                contract_address_salt: ContractAddressSalt(shash!("0x2")),
            })
        })
        .collect();
    let tx_outputs: Vec<TransactionOutput> = (0..10)
        .map(|i| {
            TransactionOutput::Deploy(DeployTransactionOutput {
                actual_fee: Fee(i as u128),
                messages_sent: vec![],
                events: vec![],
            })
        })
        .collect();

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
        .append_body(BlockNumber::new(0), &body0)?
        .append_body(BlockNumber::new(1), &body1)?
        .commit()?;

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber::new(5), &body2) {
        assert_matches!(
            err,
            StorageError::MarkerMismatch { expected, found }
        if expected == BlockNumber::new(2) && found == BlockNumber::new(5));
    } else {
        panic!("Unexpected Ok.");
    }

    writer.begin_rw_txn()?.append_body(BlockNumber::new(2), &body2)?.commit()?;

    if let Err(err) = writer.begin_rw_txn()?.append_body(BlockNumber::new(3), &body3) {
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

    // Check single transactions and outputs.
    let tx_cases = vec![
        (BlockNumber::new(0), TransactionOffsetInBlock(0), Some(0)),
        (BlockNumber::new(0), TransactionOffsetInBlock(1), None),
        (BlockNumber::new(1), TransactionOffsetInBlock(0), None),
        (BlockNumber::new(2), TransactionOffsetInBlock(0), Some(1)),
        (BlockNumber::new(2), TransactionOffsetInBlock(1), Some(2)),
        (BlockNumber::new(2), TransactionOffsetInBlock(2), None),
    ];

    for (block_number, tx_offset, original_index) in tx_cases {
        let expected_tx = original_index.map(|i| &txs[i]);
        assert_eq!(
            txn.get_transaction(TransactionIndex(block_number, tx_offset))?.as_ref(),
            expected_tx
        );
        let expected_tx_output = original_index.map(|i| &tx_outputs[i]);
        assert_eq!(
            txn.get_transaction_output(TransactionIndex(block_number, tx_offset))?.as_ref(),
            expected_tx_output
        );
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
        Some(vec![tx_outputs[0].clone()])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber::new(1))?, Some(vec![]));
    assert_eq!(
        txn.get_block_transaction_outputs(BlockNumber::new(2))?,
        Some(vec![tx_outputs[1].clone(), tx_outputs[2].clone()])
    );
    assert_eq!(txn.get_block_transaction_outputs(BlockNumber::new(3))?, None);
    Ok(())
}

#[tokio::test]
async fn revert_non_existing_body_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    if let Err(err) = writer.begin_rw_txn()?.revert_body(BlockNumber::new(5)) {
        assert_matches!(
            err,
            StorageError::InvalidRevert {
                revert_block_number,
                block_number_marker
            }
            if revert_block_number == BlockNumber::new(5) && block_number_marker == BlockNumber::new(0)
        )
    } else {
        panic!("Unexpected Ok.");
    }
    Ok(())
}

#[tokio::test]
async fn revert_last_body_success() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    writer.begin_rw_txn()?.append_body(BlockNumber::new(0), &BlockBody::default())?.commit()?;
    writer.begin_rw_txn()?.revert_body(BlockNumber::new(0))?.commit()?;
    Ok(())
}

#[tokio::test]
async fn revert_old_body_fails() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    append_2_bodies(&mut writer)?;
    if let Err(err) = writer.begin_rw_txn()?.revert_body(BlockNumber::new(0)) {
        assert_matches!(
            err,
            StorageError::InvalidRevert {
                revert_block_number,
                block_number_marker
            }
            if revert_block_number == BlockNumber::new(0) && block_number_marker == BlockNumber::new(2)
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
    assert_eq!(reader.begin_ro_txn()?.get_body_marker()?, BlockNumber::new(2));

    writer.begin_rw_txn()?.revert_body(BlockNumber::new(1))?.commit()?;
    assert_eq!(reader.begin_ro_txn()?.get_body_marker()?, BlockNumber::new(1));

    Ok(())
}

#[tokio::test]
async fn get_reverted_body_returns_none() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    append_2_bodies(&mut writer)?;

    // Verify that we can get block 1's transactions before the revert.
    assert!(reader.begin_ro_txn()?.get_block_transactions(BlockNumber::new(1))?.is_some());
    assert!(reader.begin_ro_txn()?.get_block_transaction_outputs(BlockNumber::new(1))?.is_some());

    writer.begin_rw_txn()?.revert_body(BlockNumber::new(1))?.commit()?;
    assert!(reader.begin_ro_txn()?.get_block_transactions(BlockNumber::new(1))?.is_none());
    assert!(reader.begin_ro_txn()?.get_block_transaction_outputs(BlockNumber::new(1))?.is_none());

    Ok(())
}

#[tokio::test]
async fn revert_transactions() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let body = get_test_body(10);
    writer.begin_rw_txn()?.append_body(BlockNumber::new(0), &body)?.commit()?;

    for (offset, tx_hash) in body.transactions().iter().map(|tx| tx.transaction_hash()).enumerate()
    {
        let tx_index = TransactionIndex(BlockNumber::new(0), TransactionOffsetInBlock(offset));

        assert!(reader.begin_ro_txn()?.get_transaction(tx_index)?.is_some());
        assert!(reader.begin_ro_txn()?.get_transaction_output(tx_index)?.is_some());
        assert_eq!(
            reader.begin_ro_txn()?.get_transaction_idx_by_hash(&tx_hash)?.unwrap(),
            tx_index
        );
    }

    writer.begin_rw_txn()?.revert_body(BlockNumber::new(0))?.commit()?;

    for (offset, tx_hash) in body.transactions().iter().map(|tx| tx.transaction_hash()).enumerate()
    {
        let tx_index = TransactionIndex(BlockNumber::new(0), TransactionOffsetInBlock(offset));

        assert!(reader.begin_ro_txn()?.get_transaction(tx_index)?.is_none());
        assert!(reader.begin_ro_txn()?.get_transaction_output(tx_index)?.is_none());
        assert!(reader.begin_ro_txn()?.get_transaction_idx_by_hash(&tx_hash)?.is_none());
    }

    Ok(())
}

fn append_2_bodies(writer: &mut StorageWriter) -> Result<(), anyhow::Error> {
    writer
        .begin_rw_txn()?
        .append_body(BlockNumber::new(0), &BlockBody::default())?
        .append_body(BlockNumber::new(1), &BlockBody::default())?
        .commit()?;

    Ok(())
}
