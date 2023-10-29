use std::fs::File;
use std::future::Future;
use std::io::Write;
use std::net::SocketAddr;

use rand::Rng;
use serde_json::Value as jsonVal;
use test_utils::send_request;

use crate::{
    get_last_block_number,
    get_random_block_number,
    path_in_resources,
    GET_BLOCK_TRANSACTION_COUNT_BY_HASH_WEIGHT,
    GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER_WEIGHT,
    GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_HASH_WEIGHT,
    GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_NUMBER_WEIGHT,
    GET_BLOCK_WITH_TRANSACTION_HASHES_BY_HASH_WEIGHT,
    GET_BLOCK_WITH_TRANSACTION_HASHES_BY_NUMBER_WEIGHT,
    GET_CLASS_AT_BY_HASH_WEIGHT,
    GET_CLASS_AT_BY_NUMBER_WEIGHT,
    GET_CLASS_BY_HASH_WEIGHT,
    GET_CLASS_BY_NUMBER_WEIGHT,
    GET_CLASS_HASH_AT_BY_HASH_WEIGHT,
    GET_CLASS_HASH_AT_BY_NUMBER_WEIGHT,
    GET_EVENTS_WITHOUT_ADDRESS_WEIGHT,
    GET_EVENTS_WITH_ADDRESS_WEIGHT,
    GET_NONCE_BY_HASH_WEIGHT,
    GET_NONCE_BY_NUMBER_WEIGHT,
    GET_STATE_UPDATE_BY_HASH_WEIGHT,
    GET_STATE_UPDATE_BY_NUMBER_WEIGHT,
    GET_STORAGE_AT_BY_HASH_WEIGHT,
    GET_STORAGE_AT_BY_NUMBER_WEIGHT,
    GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_HASH_WEIGHT,
    GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_NUMBER_WEIGHT,
    GET_TRANSACTION_BY_HASH_WEIGHT,
    GET_TRANSACTION_RECEIPT_WEIGHT,
    RPC_VERSION_ID,
};

// The limit on the storage size for request arguments.
const STORAGE_SIZE_IN_BYTES: usize = 7000;
// Average size of arguments to a request.
const AVERAGE_ARGS_SIZE_IN_BYTES: usize = 70;
// The number of arguments to requests we can save with the given storage size limit.
const ARGS_COUNT: usize = STORAGE_SIZE_IN_BYTES / AVERAGE_ARGS_SIZE_IN_BYTES;

// Returns the number of arguments given a weight.
const fn get_args_count(weight: usize) -> usize {
    weight * ARGS_COUNT / WEIGHT_SUM
}

// The weight of each file. The weight is the sum of the request weights which use the file content
// as arguments.
const BLOCK_NUMBER_WEIGHT: usize = GET_BLOCK_WITH_TRANSACTION_HASHES_BY_NUMBER_WEIGHT
    + GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_NUMBER_WEIGHT
    + GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER_WEIGHT
    + GET_STATE_UPDATE_BY_NUMBER_WEIGHT;
const BLOCK_HASH_WEIGHT: usize = GET_BLOCK_WITH_TRANSACTION_HASHES_BY_HASH_WEIGHT
    + GET_BLOCK_WITH_FULL_TRANSACTIONS_BY_HASH_WEIGHT
    + GET_BLOCK_TRANSACTION_COUNT_BY_HASH_WEIGHT
    + GET_STATE_UPDATE_BY_HASH_WEIGHT;
const BLOCK_NUMBER_AND_TRANSACTION_INDEX_WEIGHT: usize =
    GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_NUMBER_WEIGHT;
const BLOCK_HASH_AND_TRANSACTION_INDEX_WEIGHT: usize =
    GET_TRANSACTION_BY_BLOCK_ID_AND_INDEX_BY_HASH_WEIGHT;
const TRANSACTION_HASH_WEIGHT: usize =
    GET_TRANSACTION_BY_HASH_WEIGHT + GET_TRANSACTION_RECEIPT_WEIGHT;
const BLOCK_NUMBER_AND_CONTRACT_ADDRESS_WEIGHT: usize = GET_CLASS_AT_BY_NUMBER_WEIGHT
    + GET_CLASS_HASH_AT_BY_NUMBER_WEIGHT
    + GET_NONCE_BY_NUMBER_WEIGHT
    + GET_STORAGE_AT_BY_NUMBER_WEIGHT;
const BLOCK_HASH_AND_CONTRACT_ADDRESS_WEIGHT: usize = GET_CLASS_AT_BY_HASH_WEIGHT
    + GET_CLASS_HASH_AT_BY_HASH_WEIGHT
    + GET_NONCE_BY_HASH_WEIGHT
    + GET_STORAGE_AT_BY_HASH_WEIGHT;
const BLOCK_NUMBER_AND_CLASS_HASH_WEIGHT: usize = GET_CLASS_BY_NUMBER_WEIGHT;
const BLOCK_HASH_AND_CLASS_HASH_WEIGHT: usize = GET_CLASS_BY_HASH_WEIGHT;
const BLOCK_RANGE_AND_CONTRACT_ADDRESS_WEIGHT: usize =
    GET_EVENTS_WITH_ADDRESS_WEIGHT + GET_EVENTS_WITHOUT_ADDRESS_WEIGHT;

// The sum of the file’s weights.
const WEIGHT_SUM: usize = BLOCK_NUMBER_WEIGHT
    + BLOCK_HASH_WEIGHT
    + BLOCK_NUMBER_AND_TRANSACTION_INDEX_WEIGHT
    + BLOCK_HASH_AND_TRANSACTION_INDEX_WEIGHT
    + TRANSACTION_HASH_WEIGHT
    + BLOCK_NUMBER_AND_CONTRACT_ADDRESS_WEIGHT
    + BLOCK_HASH_AND_CONTRACT_ADDRESS_WEIGHT
    + BLOCK_NUMBER_AND_CLASS_HASH_WEIGHT
    + BLOCK_HASH_AND_CLASS_HASH_WEIGHT
    + BLOCK_RANGE_AND_CONTRACT_ADDRESS_WEIGHT;

// The number of arguments to write in a file.
const BLOCK_NUMBER_COUNT: usize = get_args_count(BLOCK_NUMBER_WEIGHT);
const BLOCK_HASH_COUNT: usize = get_args_count(BLOCK_HASH_WEIGHT);
const BLOCK_NUMBER_AND_TRANSACTION_INDEX_COUNT: usize =
    get_args_count(BLOCK_NUMBER_AND_TRANSACTION_INDEX_WEIGHT);
const BLOCK_HASH_AND_TRANSACTION_INDEX_COUNT: usize =
    get_args_count(BLOCK_HASH_AND_TRANSACTION_INDEX_WEIGHT);
const TRANSACTION_HASH_COUNT: usize = get_args_count(TRANSACTION_HASH_WEIGHT);
const BLOCK_NUMBER_AND_CONTRACT_ADDRESS_COUNT: usize =
    get_args_count(BLOCK_NUMBER_AND_CONTRACT_ADDRESS_WEIGHT);
const BLOCK_HASH_AND_CONTRACT_ADDRESS_COUNT: usize =
    get_args_count(BLOCK_HASH_AND_CONTRACT_ADDRESS_WEIGHT);
const BLOCK_NUMBER_AND_CLASS_HASH_COUNT: usize = get_args_count(BLOCK_NUMBER_AND_CLASS_HASH_WEIGHT);
const BLOCK_HASH_AND_CLASS_HASH_COUNT: usize = get_args_count(BLOCK_HASH_AND_CLASS_HASH_WEIGHT);
const BLOCK_RANGE_AND_CONTRACT_ADDRESS_COUNT: usize =
    get_args_count(BLOCK_RANGE_AND_CONTRACT_ADDRESS_WEIGHT);

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
        tokio::spawn(create_file("transaction_hash.txt", TRANSACTION_HASH_COUNT, move || {
            get_transaction_hash_args(node_socket)
        }));
    let block_hash_and_transaction_index = tokio::spawn(create_file(
        "block_hash_and_transaction_index.txt",
        BLOCK_HASH_AND_TRANSACTION_INDEX_COUNT,
        move || get_block_hash_and_transaction_index_args(node_socket),
    ));
    let block_number_and_transaction_index = tokio::spawn(create_file(
        "block_number_and_transaction_index.txt",
        BLOCK_NUMBER_AND_TRANSACTION_INDEX_COUNT,
        move || get_block_number_and_transaction_index_args(node_socket),
    ));
    let block_number_and_contract_address = tokio::spawn(create_file(
        "block_number_and_contract_address.txt",
        BLOCK_NUMBER_AND_CONTRACT_ADDRESS_COUNT,
        move || get_block_number_and_contract_address_args(node_socket),
    ));
    let block_hash_and_contract_address = tokio::spawn(create_file(
        "block_hash_and_contract_address.txt",
        BLOCK_HASH_AND_CONTRACT_ADDRESS_COUNT,
        move || get_block_hash_and_contract_address_args(node_socket),
    ));
    let block_number_and_class_hash = tokio::spawn(create_file(
        "block_number_and_class_hash.txt",
        BLOCK_NUMBER_AND_CLASS_HASH_COUNT,
        move || get_block_number_and_class_hash_args(node_socket),
    ));
    let block_hash_and_class_hash = tokio::spawn(create_file(
        "block_hash_and_class_hash.txt",
        BLOCK_HASH_AND_CLASS_HASH_COUNT,
        move || get_block_hash_and_class_hash_args(node_socket),
    ));
    let block_range_and_contract_address = tokio::spawn(create_file(
        "block_range_and_contract_address.txt",
        BLOCK_RANGE_AND_CONTRACT_ADDRESS_COUNT,
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
        block_number_and_class_hash,
        block_hash_and_class_hash,
        block_range_and_contract_address,
    )
    .unwrap();
}

// Write to a file lines with parameters to requests.
// - file_name: the file to write to.
// - params_set_count: the number of lines with parameters to write to the file.
// - get_params: a function that returns a vector with parameters to a request. The use of Fn is to
//   enable closure, and the reason get_args is async is that creating the parameters is IO bound.
pub async fn create_file<Fut>(file_name: &str, param_set_count: usize, get_params: impl Fn() -> Fut)
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
    to_write.pop();
    let mut file = File::create(path_in_resources(file_name))
        .unwrap_or_else(|err| panic!("Create file \"{file_name}\" failed.: {err}"));
    file.write_all(to_write.as_bytes()).unwrap();
}

pub async fn get_block_with_tx_hashes(node_address: SocketAddr, block_number: u64) -> jsonVal {
    let params = format!("{{ \"block_number\": {block_number} }}");
    send_request(node_address, "starknet_getBlockWithTxHashes", &params, (*RPC_VERSION_ID).as_str())
        .await
}

// Creates the file last_block_number.txt. Write to the file the last block number for the load
// test.
async fn last_block_number(node_address: SocketAddr) {
    let last_block_number =
        &send_request(node_address, "starknet_blockNumber", "", (*RPC_VERSION_ID).as_str()).await
            ["result"];
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
    let response = &send_request(
        node_address,
        "starknet_getBlockTransactionCount",
        &params,
        (*RPC_VERSION_ID).as_str(),
    )
    .await["result"];
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
    let response =
        &send_request(node_address, "starknet_getStateUpdate", &params, (*RPC_VERSION_ID).as_str())
            .await["result"]["state_diff"]["deployed_contracts"];
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

// Returns a vector with a random block number and class hash of a class which was declared
// before the block.
pub async fn get_block_number_and_class_hash_args(node_address: SocketAddr) -> Vec<String> {
    let (block_number, class_hash) = get_random_block_number_and_class_hash(node_address).await;
    // A block number which in it the class was already declared.
    let after_block_number = rand::thread_rng().gen_range(block_number..=get_last_block_number());
    vec![after_block_number.to_string(), class_hash]
}

// Returns a vector with a random block hash and class hash of a class which was declared
// before the block.
pub async fn get_block_hash_and_class_hash_args(node_address: SocketAddr) -> Vec<String> {
    let (block_number, class_hash) = get_random_block_number_and_class_hash(node_address).await;
    // A block number which in it the class was already declared.
    let after_block_number = rand::thread_rng().gen_range(block_number..=get_last_block_number());
    let after_block_hash = get_block_hash_by_block_number(after_block_number, node_address).await;
    vec![after_block_hash, class_hash]
}

// Returns a vector with a random block number and class hash of a class which was declared
// in this block.
pub async fn get_random_block_number_and_class_hash(node_address: SocketAddr) -> (u64, String) {
    loop {
        let block_number = get_random_block_number();
        let class_hash = get_random_class_hash_declared_in_block(block_number, node_address).await;
        if let Some(class_hash) = class_hash {
            return (block_number, class_hash);
        }
    }
}

// Given a block number return a random class hash which was declared in this block.
// Returns Option<String> because it is possible that no classes were declared in the given block.
pub async fn get_random_class_hash_declared_in_block(
    block_number: u64,
    node_address: SocketAddr,
) -> Option<String> {
    let params = format!("{{ \"block_number\": {block_number} }}");
    let mut declared_classes = Vec::<jsonVal>::new();
    // Cairo 1 classes.
    let classes = &mut send_request(
        node_address,
        "starknet_getStateUpdate",
        &params,
        (*RPC_VERSION_ID).as_str(),
    )
    .await["result"]["state_diff"]["declared_classes"]
        .take();
    // Cairo 1 declared classes returns as a couple of "class_hash" and "compiled_class_hash".
    let mut classes = classes
        .as_array_mut()
        .unwrap()
        .iter()
        .map(|two_hashes| two_hashes["class_hash"].clone())
        .collect();
    declared_classes.append(&mut classes);
    // Cairo 0 classes.
    let classes = &mut send_request(
        node_address,
        "starknet_getStateUpdate",
        &params,
        (*RPC_VERSION_ID).as_str(),
    )
    .await["result"]["state_diff"]["deprecated_declared_classes"]
        .take();
    declared_classes.append(classes.as_array_mut().unwrap());

    if declared_classes.is_empty() {
        return None;
    }
    let random_index = rand::thread_rng().gen_range(0..declared_classes.len());
    let class_hash = declared_classes[random_index].as_str().unwrap().to_string();
    Some(class_hash)
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
