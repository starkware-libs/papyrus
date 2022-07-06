use crate::starknet::{
    shash, BlockBody, BlockNumber, CallData, ContractAddress, DeployTransaction, Fee, StarkHash,
    Transaction, TransactionHash, TransactionIndex, TransactionVersion,
};
use crate::storage::components::block::body::BodyStorageReader;
use crate::storage::components::block::test_utils::get_test_storage;

use super::{BlockStorageError, BodyStorageWriter};

#[tokio::test]
async fn test_append_body() {
    let (reader, mut writer) = get_test_storage();

    let tx0 = Transaction::Deploy(DeployTransaction {
        transaction_hash: TransactionHash(shash!("0x100")),
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x300")),
        constructor_calldata: CallData(vec![shash!("0x400")]),
    });

    let tx1 = Transaction::Deploy(DeployTransaction {
        transaction_hash: TransactionHash(shash!("0x101")),
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x301")),
        constructor_calldata: CallData(vec![shash!("0x401")]),
    });
    let body0 = BlockBody {
        transactions: vec![tx0.clone()],
    };
    let body1 = BlockBody {
        transactions: vec![],
    };
    let body2 = BlockBody {
        transactions: vec![tx1.clone(), tx0.clone()],
    };
    let body3 = BlockBody {
        transactions: vec![tx1.clone()],
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
    writer.append_body(BlockNumber(3), &body3).unwrap();

    // Check marker.
    assert_eq!(reader.get_body_marker().unwrap(), BlockNumber(4));

    // Check transactions.
    assert_eq!(
        reader
            .get_transaction(BlockNumber(0), TransactionIndex(0))
            .unwrap()
            .as_ref(),
        Some(&tx0)
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(0), TransactionIndex(1))
            .unwrap(),
        None
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(1), TransactionIndex(0))
            .unwrap(),
        None
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(2), TransactionIndex(0))
            .unwrap()
            .as_ref(),
        Some(&tx1)
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(2), TransactionIndex(1))
            .unwrap()
            .as_ref(),
        Some(&tx0)
    );
    assert_eq!(
        reader
            .get_transaction(BlockNumber(2), TransactionIndex(2))
            .unwrap(),
        None,
    );
}
