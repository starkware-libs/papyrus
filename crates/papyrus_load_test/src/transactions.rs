use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::sync::Arc;

use goose::goose::{Transaction, TransactionFunction};
use rand::Rng;
use serde_json::{json, Value as jsonVal};

use crate::{create_request, jsonrpc_request, path_in_resources, post_jsonrpc_request};

create_get_transaction_function_with_requests_from_file! {
    get_block_with_transaction_hashes_by_number, "block_number.txt";
    get_block_with_transaction_hashes_by_hash, "block_hash.txt";
    get_block_with_full_transactions_by_number, "block_number.txt";
    get_block_with_full_transactions_by_hash, "block_hash.txt";
    get_block_transaction_count_by_number, "block_number.txt";
    get_block_transaction_count_by_hash, "block_hash.txt";
    get_state_update_by_number, "block_number.txt";
    get_state_update_by_hash, "block_hash.txt";
    get_transaction_by_block_id_and_index_by_number, "block_number_and_transaction_index.txt";
    get_transaction_by_block_id_and_index_by_hash, "block_hash_and_transaction_index.txt";
    get_transaction_by_hash, "transaction_hash.txt";
    get_transaction_receipt, "transaction_hash.txt";
    get_class_at_by_number, "block_number_and_contract_address.txt";
    get_class_at_by_hash, "block_hash_and_contract_address.txt";
    get_class_hash_at_by_number, "block_number_and_contract_address.txt";
    get_class_hash_at_by_hash, "block_hash_and_contract_address.txt";
    get_nonce_by_number, "block_number_and_contract_address.txt";
    get_nonce_by_hash, "block_hash_and_contract_address.txt";
    get_storage_at_by_number, "block_number_and_contract_address.txt";
    get_storage_at_by_hash, "block_hash_and_contract_address.txt";
    get_class_by_number, "block_number_and_class_hash.txt";
    get_class_by_hash, "block_hash_and_class_hash.txt";
    get_events_with_address, "block_range_and_contract_address.txt";
    get_events_without_address, "block_range_and_contract_address.txt";
    trace_transaction, "transaction_hash.txt";
    trace_block_transactions_by_number, "block_number.txt";
    trace_block_transactions_by_hash, "block_hash.txt";
}

pub fn block_number() -> Transaction {
    transaction_with_constant_request("blockNumber", "block_number")
}

pub fn block_hash_and_number() -> Transaction {
    transaction_with_constant_request("blockHashAndNumber", "block_hash_and_number")
}

pub fn chain_id() -> Transaction {
    transaction_with_constant_request("chainId", "chain_id")
}

pub fn syncing() -> Transaction {
    transaction_with_constant_request("syncing", "syncing")
}

fn transaction_with_constant_request(method_name: &str, transaction_name: &str) -> Transaction {
    let method = String::from("starknet_") + method_name;
    let request = jsonrpc_request(&method, json!([]));
    let func: TransactionFunction = Arc::new(move |user| {
        let request = request.clone();
        Box::pin(async move {
            post_jsonrpc_request(user, &request).await?;

            Ok(())
        })
    });
    Transaction::new(func).set_name(transaction_name)
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

// Given file_name reads the file line by line and, for each line, creates request to the node using
// convert_to_request function. Returns a vector with all the requests were created from the file.
pub(crate) fn create_requests_vector_from_file(
    file_name: &str,
    convert_to_request: fn(&str) -> jsonVal,
) -> Vec<jsonVal> {
    let file = File::open(path_in_resources(file_name)).unwrap();
    let reader = BufReader::new(file);
    let mut requests = Vec::<jsonVal>::new();
    for line in reader.lines() {
        requests.push(convert_to_request(&line.unwrap()));
    }
    requests
}

// Given [Name, "Path";] write the function:
//      pub fn Name() -> Transaction {
//          let requests = create_requests_vector("Path", create_request::Name);
//          random_request_transaction(requests).set_name(Name)
//      }
macro_rules! create_get_transaction_function_with_requests_from_file {
    () => {};
    ($name:tt, $file_name:literal; $($rest:tt)*) => {
        pub fn $name() -> Transaction {
            let requests = create_requests_vector_from_file($file_name, create_request::$name);
            random_request_transaction(requests).set_name(stringify!($name))
        }
        create_get_transaction_function_with_requests_from_file!($($rest)*);
    };
}
pub(crate) use create_get_transaction_function_with_requests_from_file;
