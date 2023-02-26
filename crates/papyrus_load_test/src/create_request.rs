use serde_json::{json, Value as jsonVal};

use crate::jsonrpc_request;

pub fn get_block_with_tx_hashes_by_number(block_number: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": block_number }]))
}

pub fn get_block_with_tx_hashes_by_hash(block_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_hash": block_hash }]))
}
