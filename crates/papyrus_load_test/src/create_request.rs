use goose::goose::GooseUser;
use serde_json::json;

use crate::{jsonrpc_request, post_jsonrpc_request, PostResult};

pub async fn get_block_with_tx_hashes_by_number(
    user: &mut GooseUser,
    block_number: u64,
) -> PostResult {
    post_jsonrpc_request(
        user,
        &jsonrpc_request(
            "starknet_getBlockWithTxHashes",
            json!([{ "block_number": block_number }]),
        ),
    )
    .await
}

pub async fn get_block_with_tx_hashes_by_hash(
    user: &mut GooseUser,
    block_hash: &str,
) -> PostResult {
    post_jsonrpc_request(
        user,
        &jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_hash": block_hash }])),
    )
    .await
}
