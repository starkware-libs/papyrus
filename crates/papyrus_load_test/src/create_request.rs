use std::str::SplitWhitespace;

use serde_json::{json, Value as jsonVal};

use crate::jsonrpc_request;

struct ArgsIter<'a> {
    iter: SplitWhitespace<'a>,
}

impl<'a> ArgsIter<'a> {
    fn new(args: &'a str) -> Self {
        ArgsIter { iter: args.split_whitespace() }
    }

    fn next_str(&mut self) -> &str {
        self.iter.next().unwrap()
    }

    fn next_u64(&mut self) -> u64 {
        self.iter.next().unwrap().parse::<u64>().unwrap()
    }
}

pub fn get_transaction_by_block_id_and_index_by_hash(args: &str) -> jsonVal {
    let mut iter = ArgsIter::new(args);
    jsonrpc_request(
        "starknet_getTransactionByBlockIdAndIndex",
        json!([ {"block_hash": iter.next_str()},  iter.next_u64()  ]),
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
