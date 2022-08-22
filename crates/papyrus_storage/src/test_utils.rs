use starknet_api::{
    shash, BlockBody, BlockHash, BlockHeader, BlockNumber, CallData, ClassHash, ContractAddress,
    ContractAddressSalt, DeployTransaction, DeployTransactionOutput, Fee, StarkHash, Transaction,
    TransactionHash, TransactionOutput, TransactionVersion,
};
use tempfile::tempdir;

use super::{open_storage, StorageReader, StorageWriter};
use crate::db::DbConfig;

pub fn get_test_config() -> DbConfig {
    let dir = tempdir().unwrap();
    DbConfig {
        path: dir.path().to_str().unwrap().to_string(),
        max_size: 1 << 35, // 32GB.
    }
}
pub fn get_test_storage() -> (StorageReader, StorageWriter) {
    let config = get_test_config();
    open_storage(config).expect("Failed to open storage.")
}

pub fn get_test_block(transaction_count: usize) -> (BlockHeader, BlockBody) {
    let header = BlockHeader {
        block_hash: BlockHash(shash!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
        )),
        block_number: BlockNumber(0),
        ..BlockHeader::default()
    };
    let mut transactions = vec![];
    let mut transaction_outputs = vec![];
    for i in 0..transaction_count {
        let transaction = Transaction::Deploy(DeployTransaction::new(
            TransactionHash(StarkHash::from_u64(i as u64)),
            TransactionVersion(shash!("0x1")),
            ClassHash(StarkHash::from_u64(i as u64)),
            ContractAddress(shash!("0x2")),
            ContractAddressSalt(shash!("0x4")),
            CallData(vec![shash!("0x3")]),
        ));
        transactions.push(transaction);
        let transaction_output =
            TransactionOutput::Deploy(DeployTransactionOutput::new(Fee::default()));
        transaction_outputs.push(transaction_output);
    }
    let body = BlockBody { transactions, transaction_outputs };
    (header, body)
}
