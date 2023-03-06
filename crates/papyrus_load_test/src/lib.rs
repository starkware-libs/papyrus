pub mod create_files;
pub mod create_request;
pub mod scenarios;
pub mod transactions;

use std::fs;

use goose::goose::{GooseUser, TransactionError};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value as jsonVal};

type PostResult = Result<jsonVal, Box<TransactionError>>;

pub async fn post_jsonrpc_request(user: &mut GooseUser, request: &jsonVal) -> PostResult {
    let response = user.post_json("", request).await?.response.map_err(|e| Box::new(e.into()))?;
    // The purpose of this struct and the line afterward is to report on failed requests.
    // The "response.json::<TransactionReceiptResponse>" deserialize the body of response to
    // TransactionReceiptResponse. If the response is an error, the result field doesn't exist in
    // the body, the deserialization will fail, and the function will return an error.
    #[derive(Deserialize)]
    struct TransactionReceiptResponse {
        result: jsonVal,
    }
    let response =
        response.json::<TransactionReceiptResponse>().await.map_err(|e| Box::new(e.into()))?;
    Ok(response.result)
}

pub fn jsonrpc_request(method: &str, params: jsonVal) -> jsonVal {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}

// Returns the last block number for which this load test is relevant.
pub fn get_last_block_number() -> u64 {
    fs::read_to_string("crates/papyrus_load_test/src/resources/last_block_number.txt")
        .unwrap()
        .parse::<u64>()
        .unwrap()
}

// Returns a random block from zero to the last block for which this load test is relevant.
pub fn get_random_block_number() -> u64 {
    let last_block = get_last_block_number();
    let mut rng = rand::thread_rng();
    rng.gen_range(0..=last_block)
}
