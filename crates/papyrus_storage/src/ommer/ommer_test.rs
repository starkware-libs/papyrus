use starknet_api::{
    shash, BlockHash, BlockHeader, StarkHash, Transaction, TransactionOffsetInBlock,
    TransactionOutput,
};

use super::OmmerStorageWriter;
use crate::test_utils::{get_test_block, get_test_storage};
use crate::{StorageError, StorageReader, StorageResult};

fn get_ommer_header(
    reader: &StorageReader,
    block_hash: BlockHash,
) -> StorageResult<Option<BlockHeader>> {
    let storage_txn = reader.begin_ro_txn()?;
    let ommer_headers_table = storage_txn.txn.open_table(&storage_txn.tables.ommer_headers)?;
    ommer_headers_table.get(&storage_txn.txn, &block_hash).map_err(StorageError::InnerError)
}

fn get_ommer_transaction(
    reader: &StorageReader,
    block_hash: BlockHash,
    offset: usize,
) -> StorageResult<Option<Transaction>> {
    let storage_txn = reader.begin_ro_txn()?;
    let ommer_transactions_table =
        storage_txn.txn.open_table(&storage_txn.tables.ommer_transactions)?;
    ommer_transactions_table
        .get(&storage_txn.txn, &(block_hash, TransactionOffsetInBlock(offset)))
        .map_err(StorageError::InnerError)
}

fn get_ommer_transaction_output(
    reader: &StorageReader,
    block_hash: BlockHash,
    offset: usize,
) -> StorageResult<Option<TransactionOutput>> {
    let storage_txn = reader.begin_ro_txn()?;
    let ommer_transaction_outputs_table =
        storage_txn.txn.open_table(&storage_txn.tables.ommer_transaction_outputs)?;
    ommer_transaction_outputs_table
        .get(&storage_txn.txn, &(block_hash, TransactionOffsetInBlock(offset)))
        .map_err(StorageError::InnerError)
}

#[test]
fn insert_ommer_block() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let hash = BlockHash::new(shash!("0x0"));
    let block = get_test_block(7);

    assert!(get_ommer_header(&reader, hash)?.is_none());
    assert!(get_ommer_transaction(&reader, hash, 0)?.is_none());
    assert!(get_ommer_transaction_output(&reader, hash, 0)?.is_none());

    writer
        .begin_rw_txn()?
        .insert_ommer_block(hash, block.header.clone(), block.body.clone())?
        .commit()?;

    if let Some(ommer_header) = get_ommer_header(&reader, hash)? {
        assert_eq!(ommer_header, block.header);
    } else {
        panic!("Unexpected none")
    }

    let tx_iter = block.body.transactions().iter();
    let tx_output_iter = block.body.transaction_outputs().iter();
    for (offset, (tx, tx_output)) in tx_iter.zip(tx_output_iter).enumerate() {
        if let Some(ommer_transaction) = get_ommer_transaction(&reader, hash, offset)? {
            assert_eq!(ommer_transaction, *tx);
        } else {
            panic!("Unexpected none")
        }
        if let Some(ommer_transaction_output) = get_ommer_transaction_output(&reader, hash, offset)?
        {
            assert_eq!(ommer_transaction_output, *tx_output);
        } else {
            panic!("Unexpected none")
        }
    }

    Ok(())
}
