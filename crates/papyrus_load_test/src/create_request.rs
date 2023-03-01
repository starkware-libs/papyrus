use serde_json::{json, Value as jsonVal};
use std::str::SplitWhitespace;

use crate::jsonrpc_request;

struct ArgsIter<'a>{
    iter: SplitWhitespace<'a>,
}

impl<'a> ArgsIter<'a>{
    fn new(args: &'a str) -> Self{
        ArgsIter { iter: args.split_whitespace() }
    }

    fn next_str(&mut self) -> &str{
        self.iter.next().unwrap()
    }

    fn next_u64(&mut self) ->u64{
        self.iter.next().unwrap().parse::<u64>().unwrap()
    }
}

pub fn get_block_with_transaction_hashes_by_number(args: &str) -> jsonVal {
    let mut iter=ArgsIter::new(args);
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": iter.next_u64() }]))
}

pub fn get_block_with_transaction_hashes_by_hash(args: &str) -> jsonVal {
    let mut iter=ArgsIter::new(args);
    jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_hash": iter.next_str() }]))
}

