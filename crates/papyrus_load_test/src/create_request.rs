use std::str::SplitWhitespace;

use serde_json::{json, Value as jsonVal};

use crate::jsonrpc_request;

// TODO(dvir): consider adding more variations to get_events requests.
// Chunk size for get_events requests.
const CHUNK_SIZE: usize = 100;
pub fn get_events_with_address(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let from_block = arg_iter.next_u64();
    let to_block = arg_iter.next_u64();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getEvents",
        json!([{"from_block":{"block_number": from_block}, "to_block":{"block_number": to_block}, 
        "chunk_size": CHUNK_SIZE, "address": contract_address, "keys": []}]),
    )
}

pub fn get_events_without_address(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let from_block = arg_iter.next_u64();
    let to_block = arg_iter.next_u64();
    jsonrpc_request(
        "starknet_getEvents",
        json!([{"from_block":{"block_number": from_block}, "to_block":{"block_number": to_block}, 
        "chunk_size": CHUNK_SIZE, "keys": []}]),
    )
}

pub fn get_class_by_number(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_number = arg_iter.next_u64();
    let class_hash = arg_iter.next_str();
    jsonrpc_request("starknet_getClass", json!([{ "block_number": block_number }, class_hash]))
}

pub fn get_class_by_hash(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_hash = arg_iter.next_str();
    let class_hash = arg_iter.next_str();
    jsonrpc_request("starknet_getClass", json!([{ "block_hash": block_hash }, class_hash]))
}

pub fn get_storage_at_by_number(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_number = arg_iter.next_u64();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getStorageAt",
        json!([contract_address, "0x0", { "block_number": block_number }]),
    )
}

pub fn get_storage_at_by_hash(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_hash = arg_iter.next_str();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getStorageAt",
        json!([contract_address, "0x0", { "block_hash": block_hash }]),
    )
}

pub fn get_nonce_by_number(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_number = arg_iter.next_u64();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getNonce",
        json!([{ "block_number": block_number }, contract_address]),
    )
}

pub fn get_nonce_by_hash(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_hash = arg_iter.next_str();
    let contract_address = arg_iter.next_str();
    jsonrpc_request("starknet_getNonce", json!([{ "block_hash": block_hash }, contract_address]))
}

pub fn get_class_hash_at_by_number(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_number = arg_iter.next_u64();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getClassHashAt",
        json!([{ "block_number": block_number }, contract_address]),
    )
}

pub fn get_class_hash_at_by_hash(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_hash = arg_iter.next_str();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getClassHashAt",
        json!([{ "block_hash": block_hash }, contract_address]),
    )
}

pub fn get_class_at_by_number(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_number = arg_iter.next_u64();
    let contract_address = arg_iter.next_str();
    jsonrpc_request(
        "starknet_getClassAt",
        json!([{ "block_number": block_number }, contract_address]),
    )
}

pub fn get_class_at_by_hash(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_hash = arg_iter.next_str();
    let contract_address = arg_iter.next_str();
    jsonrpc_request("starknet_getClassAt", json!([{ "block_hash": block_hash }, contract_address]))
}

pub fn get_transaction_by_block_id_and_index_by_hash(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_hash = arg_iter.next_str();
    let transaction_index = arg_iter.next_u64();
    jsonrpc_request(
        "starknet_getTransactionByBlockIdAndIndex",
        json!([{ "block_hash": block_hash }, transaction_index]),
    )
}

pub fn get_transaction_by_block_id_and_index_by_number(args: &str) -> jsonVal {
    let mut arg_iter = ArgsIter::new(args);
    let block_number = arg_iter.next_u64();
    let transaction_index = arg_iter.next_u64();
    jsonrpc_request(
        "starknet_getTransactionByBlockIdAndIndex",
        json!([{ "block_number": block_number }, transaction_index]),
    )
}

pub fn get_block_with_transaction_hashes_by_number(block_number: &str) -> jsonVal {
    jsonrpc_request(
        "starknet_getBlockWithTxHashes",
        json!([{ "block_number": block_number.parse::<u64>().unwrap() }]),
    )
}

pub fn get_block_with_transaction_hashes_by_hash(block_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_hash": block_hash }]))
}

pub fn get_block_with_full_transactions_by_number(block_number: &str) -> jsonVal {
    jsonrpc_request(
        "starknet_getBlockWithTxs",
        json!([{ "block_number": block_number.parse::<u64>().unwrap() }]),
    )
}

pub fn get_block_with_full_transactions_by_hash(block_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockWithTxs", json!([{ "block_hash": block_hash }]))
}

pub fn get_block_transaction_count_by_number(block_number: &str) -> jsonVal {
    jsonrpc_request(
        "starknet_getBlockTransactionCount",
        json!([{ "block_number": block_number.parse::<u64>().unwrap() }]),
    )
}

pub fn get_block_transaction_count_by_hash(block_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getBlockTransactionCount", json!([{ "block_hash": block_hash }]))
}

pub fn get_state_update_by_number(block_number: &str) -> jsonVal {
    jsonrpc_request(
        "starknet_getStateUpdate",
        json!([{ "block_number": block_number.parse::<u64>().unwrap() }]),
    )
}

pub fn get_state_update_by_hash(block_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getStateUpdate", json!([{ "block_hash": block_hash }]))
}

pub fn get_transaction_by_hash(transaction_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getTransactionByHash", json!([transaction_hash]))
}

pub fn get_transaction_receipt(transaction_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_getTransactionReceipt", json!([transaction_hash]))
}

pub fn trace_transaction(transaction_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_traceTransaction", json!([transaction_hash]))
}

pub fn trace_block_transactions_by_number(block_number: &str) -> jsonVal {
    jsonrpc_request(
        "starknet_traceBlockTransactions",
        json!([{ "block_number": block_number.parse::<u64>().unwrap() }]),
    )
}

pub fn trace_block_transactions_by_hash(block_hash: &str) -> jsonVal {
    jsonrpc_request("starknet_traceBlockTransactions", json!([{ "block_hash": block_hash }]))
}

// This struct is for iterating over the args string.
struct ArgsIter<'a> {
    iter: SplitWhitespace<'a>,
}

impl<'a> ArgsIter<'a> {
    fn new(args: &'a str) -> Self {
        ArgsIter { iter: args.split_whitespace() }
    }

    // Returns the next argument as &str.
    fn next_str(&mut self) -> String {
        self.iter.next().unwrap().to_string()
    }

    // Returns the next argument as u64.
    fn next_u64(&mut self) -> u64 {
        self.iter.next().unwrap().parse::<u64>().unwrap()
    }
}
