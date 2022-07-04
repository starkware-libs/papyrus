use crate::starknet::{
    BlockBody, BlockNumber, CallData, ContractAddress, DeployTransaction, Fee, StarkHash,
    Transaction, TransactionHash, TransactionOffsetInBlock, TransactionSignature,
    TransactionVersion,
};
use crate::storage::components::block::body::BodyStorageReader;
use crate::storage::components::block::test_utils::get_test_storage;

use super::{BlockStorageError, BodyStorageWriter};

#[tokio::test]
async fn test_append_body() {
    let (reader, mut writer) = get_test_storage();

    let txs: Vec<Transaction> = (0..10)
        .map(|i| {
            Transaction::Deploy(DeployTransaction {
                transaction_hash: TransactionHash(StarkHash::from_u64(i as u64)),
                max_fee: Fee(100),
                version: TransactionVersion(1),
                signature: TransactionSignature(vec![StarkHash::from_u64(i as u64)]),
                contract_address: ContractAddress(StarkHash::from_u64(i as u64)),
                constructor_calldata: CallData(vec![StarkHash::from_u64(i as u64)]),
            })
        })
        .collect();

    let body0 = BlockBody {
        transactions: vec![txs[0].clone()],
    };
    let body1 = BlockBody {
        transactions: vec![],
    };
    let body2 = BlockBody {
        transactions: vec![txs[1].clone(), txs[2].clone()],
    };
    let body3 = BlockBody {
        transactions: vec![txs[3].clone(), txs[0].clone()],
    };
    writer.append_body(BlockNumber(0), &body0).unwrap();
    writer.append_body(BlockNumber(1), &body1).unwrap();

    // Check for MarkerMismatch error  when trying to append the wrong block number.
    assert_matches!(
        writer.append_body(BlockNumber(5), &body2).unwrap_err(),
        BlockStorageError::MarkerMismatch {
            expected: BlockNumber(2),
            found: BlockNumber(5)
        }
    );

    writer.append_body(BlockNumber(2), &body2).unwrap();

    assert_matches!(
        writer.append_body(BlockNumber(3), &body3).unwrap_err(),
        BlockStorageError::TransactionHashAlreadyExists {
            tx_hash,
            block_number: BlockNumber(3),
            tx_offset_in_block: TransactionOffsetInBlock(1)
        } if tx_hash == txs[0].transaction_hash()
    );

    // Check marker.
    assert_eq!(reader.get_body_marker().unwrap(), BlockNumber(3));

    // Check transactions.
    assert_eq!(
        reader
            .get_transaction(BlockNumber(0), TransactionOffsetInBlock(0))
            .unwrap()
            .as_ref(),
        Some(&txs[0])
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(0), TransactionOffsetInBlock(1))
            .unwrap(),
        None
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(1), TransactionOffsetInBlock(0))
            .unwrap(),
        None
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(2), TransactionOffsetInBlock(0))
            .unwrap()
            .as_ref(),
        Some(&txs[1])
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(2), TransactionOffsetInBlock(1))
            .unwrap()
            .as_ref(),
        Some(&txs[2])
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(2), TransactionOffsetInBlock(2))
            .unwrap(),
        None,
    );

    // Check transaction hash.
    // Check block hash.
    assert_eq!(
        reader
            .get_transaction_idx_by_hash(&txs[0].transaction_hash())
            .unwrap(),
        Some((BlockNumber(0), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        reader
            .get_transaction_idx_by_hash(&txs[1].transaction_hash())
            .unwrap(),
        Some((BlockNumber(2), TransactionOffsetInBlock(0)))
    );
    assert_eq!(
        reader
            .get_transaction_idx_by_hash(&txs[2].transaction_hash())
            .unwrap(),
        Some((BlockNumber(2), TransactionOffsetInBlock(1)))
    );
}
