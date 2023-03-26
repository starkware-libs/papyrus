use std::fs::File;
use std::future::Future;
use std::io::Write;
use std::net::SocketAddr;

use rand::Rng;
use serde_json::Value as jsonVal;
use test_utils::send_request;

use crate::{get_random_block_number, path_in_resources};

// Currently, those numbers are random; we will decide how to choose them
// in the future.
// The number of block numbers to write.
const BLOCK_NUMBER_COUNT: u32 = 5;
// The number of block hashes.
const BLOCK_HASH_COUNT: u32 = 5;

// Creates the files to run the load test.
pub async fn create_files(node_address: &str) {
    let node_socket = node_address.parse::<SocketAddr>().unwrap();
    last_block_number(node_socket).await;
    let block_number =
        tokio::spawn(create_file("block_number.txt", BLOCK_NUMBER_COUNT, get_block_number_args));
    let block_hash = tokio::spawn(create_file("block_hash.txt", BLOCK_HASH_COUNT, move || {
        get_block_hash_args(node_socket)
    }));
    let transaction_hash =
        tokio::spawn(create_file("transaction_hash.txt", BLOCK_HASH_COUNT, move || {
            get_transaction_hash_args(node_socket)
        }));
    tokio::try_join!(block_number, block_hash, transaction_hash).unwrap();
}

// Write to a file lines with parameters to requests.
// - file_name: the file to write to.
// - params_set_count: the number of lines with parameters to write to the file.
// - get_params: a function that returns a vector with parameters to a request. The use of Fn is to
//   enable closure, and the reason get_args is async is that creating the parameters is IO bound.
pub async fn create_file<Fut>(file_name: &str, param_set_count: u32, get_params: impl Fn() -> Fut)
where
    Fut: Future<Output = Vec<String>>,
{
    let mut to_write = String::new();
    for _ in 0..param_set_count {
        for arg in get_params().await {
            to_write.push_str(&arg);
            to_write.push(' ');
        }
        to_write.pop().unwrap();
        to_write.push('\n');
    }
    // Remove the last '\n'.
    to_write.pop().unwrap();
    let mut file =
        File::create(path_in_resources(file_name)).expect("Create file \"{file_name}\" failed.");
    file.write_all(to_write.as_bytes()).unwrap();
}

pub async fn get_block_with_tx_hashes(node_address: SocketAddr, block_number: u64) -> jsonVal {
    let params = format!("{{ \"block_number\": {block_number} }}");
    send_request(node_address, "starknet_getBlockWithTxHashes", &params).await
}

// Creates the file last_block_number.txt. Write to the file the last block number for the load
// test.
async fn last_block_number(node_address: SocketAddr) {
    let last_block_number = &send_request(node_address, "starknet_blockNumber", "").await["result"];
    let mut file = File::create(path_in_resources("last_block_number.txt")).unwrap();
    file.write_all(last_block_number.to_string().as_bytes()).unwrap();
}

// Returns a vector with a random block number.
pub async fn get_block_number_args() -> Vec<String> {
    vec![get_random_block_number().to_string()]
}

pub async fn get_block_hash_by_block_number(block_number: u64, node_address: SocketAddr) -> String {
    let response =
        &get_block_with_tx_hashes(node_address, block_number).await["result"]["block_hash"];
    let block_hash = match response {
        jsonVal::String(block_hash) => block_hash,
        _ => unreachable!(),
    };
    block_hash.to_string()
}

// Returns a vector with a random block hash.
pub async fn get_block_hash_args(node_address: SocketAddr) -> Vec<String> {
    let block_number = get_random_block_number();
    let block_hash = get_block_hash_by_block_number(block_number, node_address).await;
    vec![block_hash]
}

// Returns a vector with a random transaction hash.
pub async fn get_transaction_hash_args(node_address: SocketAddr) -> Vec<String> {
    let block_number = get_random_block_number();
    let response =
        &get_block_with_tx_hashes(node_address, block_number).await["result"]["transactions"];
    let trans_list = match response {
        jsonVal::Array(transactions) => transactions,
        _ => unreachable!("The gateway returns the transaction hashes as a vector."),
    };
    let trans_index = rand::thread_rng().gen_range(0..trans_list.len());
    let trans_hash = match &trans_list[trans_index] {
        jsonVal::String(trans_hash) => trans_hash,
        _ => unreachable!("The gateway transaction hash as a String."),
    };
    vec![trans_hash.to_string()]
}
