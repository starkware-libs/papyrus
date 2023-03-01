use serde_json::{json, Value as jsonVal};

use crate::jsonrpc_request;

pub fn get_block_with_transaction_hashes_by_number(args: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": args.parse::<u64>().unwrap() }]))
}

pub fn get_block_with_transaction_hashes_by_hash(args: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_hash": args }]))
}
