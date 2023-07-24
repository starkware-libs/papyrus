use jsonrpsee::types::ErrorObjectOwned;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::StorageTxn;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;

use crate::api::JsonRpcError;
use crate::internal_server_error;

pub fn get_block_txs_by_number<
    Mode: TransactionKind,
    Transaction: From<starknet_api::transaction::Transaction>,
>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<Transaction>, ErrorObjectOwned> {
    let transactions = txn
        .get_block_transactions(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(transactions.into_iter().map(|(tx, _execution_status)| Transaction::from(tx)).collect())
}

pub fn get_block_tx_hashes_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<TransactionHash>, ErrorObjectOwned> {
    let transaction_hashes = txn
        .get_block_transaction_hashes(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(transaction_hashes)
}
