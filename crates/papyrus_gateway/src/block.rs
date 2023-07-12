use jsonrpsee::types::ErrorObjectOwned;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageTxn;
use starknet_api::block::BlockNumber;

use crate::api::JsonRpcError;
use crate::internal_server_error;

pub fn get_block_header_by_number<
    Mode: TransactionKind,
    BlockHeader: From<starknet_api::block::BlockHeader>,
>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockHeader, ErrorObjectOwned> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(BlockHeader::from(header))
}
