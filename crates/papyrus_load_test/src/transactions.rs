use std::sync::Arc;

use goose::goose::{Transaction, TransactionFunction};
use rand::Rng;
use serde_json::{json, Value as jsonVal};

use crate::{jsonrpc_request, post_jsonrpc_request};

pub fn block_by_number() -> Transaction {
    let requests = vec![
        jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": 0 }])),
        jsonrpc_request("starknet_getBlockWithTxHashes", json!([{ "block_number": 1 }])),
    ];
    random_request_transaction(requests)
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
    random_request_transaction(requests)
}

// Returns a Transaction that each call choose a random request from the requests vector
// and sends it to the node.
fn random_request_transaction(requests: Vec<jsonVal>) -> Transaction {
    let func: TransactionFunction = Arc::new(move |user| {
        let index: usize = rand::thread_rng().gen_range(0..requests.len());
        let req = requests[index].clone();
        Box::pin(async move {
            post_jsonrpc_request(user, &req).await?;

            Ok(())
        })
    });
    Transaction::new(func)
}
