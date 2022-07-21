use assert_matches::assert_matches;
use starknet_api::{
    shash, BlockBody, BlockNumber, CallData, ClassHash, ContractAddress, ContractAddressSalt,
    DeployTransaction, StarkHash, Transaction, TransactionHash, TransactionOffsetInBlock,
    TransactionVersion,
};

use super::{BodyStorageReader, BodyStorageWriter, StorageError};
use crate::storage::test_utils::get_test_storage;

#[tokio::test]
async fn test_append_body() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();

    let txs: Vec<Transaction> = (0..10)
        .map(|i| {
            Transaction::Deploy(DeployTransaction {
                transaction_hash: TransactionHash(StarkHash::from_u64(i as u64)),
                version: TransactionVersion(shash!("0x1")),
                contract_address: ContractAddress(StarkHash::from_u64(i as u64)),
                constructor_calldata: CallData(vec![StarkHash::from_u64(i as u64)]),
                class_hash: ClassHash(StarkHash::from_u64(i as u64)),
                contract_address_salt: ContractAddressSalt(shash!("0x2")),
            })
        })
        .collect();

    let body0 = BlockBody { transactions: vec![txs[0].clone()] };
    let body1 = BlockBody { transactions: vec![] };
    let body2 = BlockBody { transactions: vec![txs[1].clone(), txs[2].clone()] };
    let body3 = BlockBody { transactions: vec![txs[3].clone(), txs[0].clone()] };
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

    // Check single transactions.
    assert_eq!(
        txn.get_transaction(BlockNumber(0), TransactionOffsetInBlock(0))?.as_ref(),
        Some(&txs[0])
    );
    assert_eq!(txn.get_transaction(BlockNumber(0), TransactionOffsetInBlock(1))?, None);
    assert_eq!(txn.get_transaction(BlockNumber(1), TransactionOffsetInBlock(0))?, None);
    assert_eq!(
        txn.get_transaction(BlockNumber(2), TransactionOffsetInBlock(0))?.as_ref(),
        Some(&txs[1])
    );
    assert_eq!(
        txn.get_transaction(BlockNumber(2), TransactionOffsetInBlock(1))?.as_ref(),
        Some(&txs[2])
    );
    assert_eq!(txn.get_transaction(BlockNumber(2), TransactionOffsetInBlock(2))?, None,);

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
    Ok(())
}
