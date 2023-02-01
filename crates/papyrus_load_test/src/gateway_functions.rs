use goose::goose::GooseUser;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::{post_jsonrpc_request, MethodResult};

pub async fn get_block_with_tx_hashes_by_number<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_number: u64,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockWithTxHashes",
        json!([{ "block_number": block_number }]),
    )
    .await
}

pub async fn get_block_with_tx_hashes_by_hash<T: DeserializeOwned>(
    user: &mut GooseUser,
    block_hash: &str,
) -> MethodResult<T> {
    post_jsonrpc_request(
        user,
        "starknet_getBlockWithTxHashes",
        json!([{ "block_hash": block_hash }]),
    )
    .await
}
