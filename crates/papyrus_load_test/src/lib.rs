#![allow(clippy::unwrap_used)]
// config compiler to support no_coverage feature when running coverage in nightly mode within this
// crate
#![cfg_attr(coverage_nightly, feature(no_coverage))]

pub mod create_files;
pub mod create_request;
#[cfg(test)]
mod precision_test;
pub mod scenarios;
pub mod transactions;

use std::{env, fs};

use goose::goose::{GooseUser, TransactionError};
use once_cell::sync::{Lazy, OnceCell};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value as jsonVal};

type PostResult = Result<jsonVal, Box<TransactionError>>;

pub async fn post_jsonrpc_request(user: &mut GooseUser, request: &jsonVal) -> PostResult {
    let version_id = &*RPC_VERSION_ID;
    let response = user
        .post_json(&format!("/rpc/{version_id}"), request)
        .await?
        .response
        .map_err(|e| Box::new(e.into()))?;
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

static LAST_BLOCK_NUMBER: OnceCell<u64> = OnceCell::new();
// Returns the last block number for which this load test is relevant.
pub fn get_last_block_number() -> u64 {
    *LAST_BLOCK_NUMBER.get_or_init(|| {
        fs::read_to_string(path_in_resources("last_block_number.txt"))
            .unwrap()
            .parse::<u64>()
            .unwrap()
    })
}

// Returns a random block from zero to the last block for which this load test is relevant.
pub fn get_random_block_number() -> u64 {
    let last_block = get_last_block_number();
    let mut rng = rand::thread_rng();
    rng.gen_range(0..=last_block)
}

// Returns the path to the file_name inside the resources folder in payprus_loadtest module.
pub fn path_in_resources(file_name: &str) -> String {
    env::var("CARGO_MANIFEST_DIR").unwrap() + "/resources/" + file_name
}

// TODO(dvir): update those number with real statics after the node will be in production.
// Weight for each request to the node.
const BLOCK_HASH_AND_NUMBER_WEIGHT: usize = 10;
const BLOCK_NUMBER_WEIGHT: usize = 10;
const CHAIN_ID_WEIGHT: usize = 10;
const GET_BLOCK_TRANSACTION_COUNT_BY_HASH_WEIGHT: usize = 10;
const GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER_WEIGHT: usize = 10;
const GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_HASH_WEIGHT: usize = 10;
const GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_NUMBER_WEIGHT: usize = 10;
const GET_BLOCK_WITH_TRANSACTION_HASHES_BY_HASH_WEIGHT: usize = 10;
const GET_BLOCK_WITH_TRANSACTION_HASHES_BY_NUMBER_WEIGHT: usize = 10;
const GET_CLASS_AT_BY_HASH_WEIGHT: usize = 10;
const GET_CLASS_AT_BY_NUMBER_WEIGHT: usize = 10;
const GET_CLASS_BY_HASH_WEIGHT: usize = 10;
const GET_CLASS_BY_NUMBER_WEIGHT: usize = 10;
const GET_CLASS_HASH_AT_BY_HASH_WEIGHT: usize = 10;
const GET_CLASS_HASH_AT_BY_NUMBER_WEIGHT: usize = 10;
const GET_EVENTS_WITHOUT_ADDRESS_WEIGHT: usize = 10;
const GET_EVENTS_WITH_ADDRESS_WEIGHT: usize = 10;
const GET_NONCE_BY_HASH_WEIGHT: usize = 10;
const GET_NONCE_BY_NUMBER_WEIGHT: usize = 10;
const GET_STATE_UPDATE_BY_HASH_WEIGHT: usize = 10;
const GET_STATE_UPDATE_BY_NUMBER_WEIGHT: usize = 10;
const GET_STORAGE_AT_BY_HASH_WEIGHT: usize = 10;
const GET_STORAGE_AT_BY_NUMBER_WEIGHT: usize = 10;
const GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_HASH_WEIGHT: usize = 10;
const GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_NUMBER_WEIGHT: usize = 10;
const GET_TRANSACTION_BY_HASH_WEIGHT: usize = 10;
const GET_TRANSACTION_RECEIPT_WEIGHT: usize = 10;
const SYNCING_WEIGHT: usize = 10;

static RPC_VERSION_ID: Lazy<String> = Lazy::new(|| match std::env::var("VERSION_ID") {
    Ok(version_id) => version_id,
    Err(_) => unreachable!("VERSION_ID environment variable is not set"),
});
