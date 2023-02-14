use std::sync::Arc;

use goose::goose::{Transaction, TransactionFunction};
use serde_json::json;

use crate::{jsonrpc_request, post_jsonrpc_request};

pub fn block_by_number() -> Transaction {
    let requests = vec![
        jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": 0 }])),
        jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": 1 }])),
 
    ];
    serial_request_transaction(requests)
}

pub fn block_by_hash() -> Transaction {
    let requests = vec![
        jsonrpc_request(
            "starknet_getBlockWithTxHashes",
            json!([{ "block_hash": "0x47c3637b57c2b079b93c61539950c17e868a28f46cdef28f88521067f21e943" }]),
        ),
        jsonrpc_request(
            "starknet_getBlockWithTxHashes",
            json!([{ "block_hash": "0x2a70fb03fe363a2d6be843343a1d81ce6abeda1e9bd5cc6ad8fa9f45e30fdeb" }]),
        ),
    ];
    serial_request_transaction(requests)
}

fn serial_request_transaction(requests: Vec<serde_json::Value>) -> Transaction {
    let func: TransactionFunction = Arc::new(move |user| {
        let requests = requests.to_owned();
        Box::pin(async move {
            for req in requests.iter() {
                post_jsonrpc_request(user, req).await?;
            }
            Ok(())
        })
    });
    Transaction::new(func)
}
