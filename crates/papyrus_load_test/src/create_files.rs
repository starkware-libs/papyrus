use std::fs::File;
use std::future::Future;
use std::io::Write;
use std::net::SocketAddr;

use rand::Rng;
use serde_json::Value as jsonVal;
use test_utils::send_request;

use crate::{get_last_block_number, get_random_block_number, path_in_resources};

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
    let block_hash_and_transaction_index = tokio::spawn(create_file(
        "block_hash_and_transaction_index.txt",
        BLOCK_HASH_COUNT,
        move || get_block_hash_and_transaction_index_args(node_socket),
    ));
    let block_number_and_transaction_index = tokio::spawn(create_file(
        "block_number_and_transaction_index.txt",
        BLOCK_HASH_COUNT,
        move || get_block_number_and_transaction_index_args(node_socket),
    ));
    let block_number_and_contract_address = tokio::spawn(create_file(
        "block_number_and_contract_address.txt",
        BLOCK_HASH_COUNT,
        move || get_block_number_and_contract_address_args(node_socket),
    ));
    let block_hash_and_contract_address = tokio::spawn(create_file(
        "block_hash_and_contract_address.txt",
        BLOCK_HASH_COUNT,
        move || get_block_hash_and_contract_address_args(node_socket),
    ));
    let block_range_and_contract_address = tokio::spawn(create_file(
        "block_range_and_contract_address.txt",
        BLOCK_HASH_COUNT,
        move || get_block_range_and_contract_address_args(node_socket),
    ));
    tokio::try_join!(
        block_number,
        block_hash,
        transaction_hash,
        block_hash_and_transaction_index,
        block_number_and_transaction_index,
        block_number_and_contract_address,
        block_hash_and_contract_address,
        block_range_and_contract_address
    )
    .unwrap();
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

// Given block number returns the number of transactions in this block.
pub async fn get_transaction_count_by_block_number(
    block_number: u64,
    node_address: SocketAddr,
) -> u64 {
    let params = format!("{{ \"block_number\": {block_number} }}");
    let response =
        &send_request(node_address, "starknet_getBlockTransactionCount", &params).await["result"];
    let trans_count = match response {
        jsonVal::Number(count) => count,
        _ => unreachable!(),
    };
    trans_count.as_u64().unwrap()
}

// Returns a vector with a random block hash and transaction index in this block.
pub async fn get_block_hash_and_transaction_index_args(node_address: SocketAddr) -> Vec<String> {
    let block_number = get_random_block_number();
    let block_hash = get_block_hash_by_block_number(block_number, node_address).await;
    let trans_count = get_transaction_count_by_block_number(block_number, node_address).await;
    let random_index = rand::thread_rng().gen_range(0..trans_count);
    vec![block_hash, random_index.to_string()]
}

// Returns a vector with a random block number and transaction index in this block.
pub async fn get_block_number_and_transaction_index_args(node_address: SocketAddr) -> Vec<String> {
    let block_number = get_random_block_number();
    let trans_count = get_transaction_count_by_block_number(block_number, node_address).await;
    let random_index = rand::thread_rng().gen_range(0..trans_count);
    vec![block_number.to_string(), random_index.to_string()]
}

// Returns a vector with a random block number and contract address of a contract which was deployed
// before the block.
pub async fn get_block_number_and_contract_address_args(node_address: SocketAddr) -> Vec<String> {
    let (block_number, contract_address) =
        get_random_block_number_and_contract_address(node_address).await;
    // A block number which in it the contract was already deployed.
    let after_block_number = rand::thread_rng().gen_range(block_number..=get_last_block_number());
    vec![after_block_number.to_string(), contract_address]
}

// Returns a vector with a random block hash and contract address of a contract which was deployed
// before the block.
pub async fn get_block_hash_and_contract_address_args(node_address: SocketAddr) -> Vec<String> {
    let (block_number, contract_address) =
        get_random_block_number_and_contract_address(node_address).await;
    // A block number which in it the contract was already deployed.
    let after_block_number = rand::thread_rng().gen_range(block_number..=get_last_block_number());
    let after_block_hash = get_block_hash_by_block_number(after_block_number, node_address).await;
    vec![after_block_hash, contract_address]
}

// Returns a vector with a random block number and contract address of a contract which was deployed
// in this block.
pub async fn get_random_block_number_and_contract_address(
    node_address: SocketAddr,
) -> (u64, String) {
    loop {
        let block_number = get_random_block_number();
        let contract_address =
            get_random_contract_address_deployed_in_block(block_number, node_address).await;
        if let Some(contract_address) = contract_address {
            return (block_number, contract_address);
        }
    }
}

// Given a block number return a random contract address which was deployed in this block.
// Returns Option<String> because it is possible that no contracts were deployed in the given block.
pub async fn get_random_contract_address_deployed_in_block(
    block_number: u64,
    node_address: SocketAddr,
) -> Option<String> {
    let params = format!("{{ \"block_number\": {block_number} }}");
    let response = &send_request(node_address, "starknet_getStateUpdate", &params).await["result"]
        ["state_diff"]["deployed_contracts"];
    let contract_list = match response {
        jsonVal::Array(contract_list) => contract_list,
        _ => unreachable!("The gateway returns the deployed contracts as a vector."),
    };
    // In case no contracts was deployed in the block.
    if contract_list.is_empty() {
        return None;
    }
    let random_index = rand::thread_rng().gen_range(0..contract_list.len());
    let contract_address = match &contract_list[random_index] {
        jsonVal::Object(contract_list) => &contract_list["address"],
        _ => unreachable!(
            "The gateway returns a deployed contracts as a mapping from address to contract \
             address."
        ),
    };
    let contract_address = match contract_address {
        jsonVal::String(contract_address) => contract_address,
        _ => unreachable!("The gateway returns a deployed contracts address as a String."),
    };
    Some(contract_address.to_string())
}

// Returns a vector with a block range (from_block_number, to_block_number) and contract address of
// a contract that was already deployed in this range.
pub async fn get_block_range_and_contract_address_args(node_address: SocketAddr) -> Vec<String> {
    let (block_number, contract_address) =
        get_random_block_number_and_contract_address(node_address).await;
    let from_block = rand::thread_rng().gen_range(block_number..=get_last_block_number());
    let to_block = rand::thread_rng().gen_range(from_block..=get_last_block_number());
    vec![from_block.to_string(), to_block.to_string(), contract_address]
}
