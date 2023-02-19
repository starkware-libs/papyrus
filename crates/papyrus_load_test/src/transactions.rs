use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::sync::Arc;

use goose::goose::{Transaction, TransactionFunction};
use rand::Rng;
use serde_json::Value as jsonVal;

use crate::{create_request, post_jsonrpc_request};

pub fn get_block_with_tx_hashes_by_number() -> Transaction {
    let requests = vec![
        create_request::get_block_with_tx_hashes_by_number(0),
        create_request::get_block_with_tx_hashes_by_number(1),
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

fn create_requests_vector(path: &str, convert_to_request: fn(String) -> jsonVal) -> Vec<jsonVal> {
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);
    let mut requests = Vec::<jsonVal>::new();
    for line in reader.lines() {
        requests.push(convert_to_request(line.unwrap()));
    }
    requests
}

// Given [Name; "Path"] write the function:
// pub fn Name() -> Transaction {
//     let requests = create_requests_vector("Path", create_request::Name);
//     random_request_transaction(requests)
// }
macro_rules! create_read_from_file_transaction {
    () => {};
    ($name:tt, $file_name:literal; $($rest:tt)*) => {
        pub fn $name() -> Transaction {
            let requests = create_requests_vector($file_name, create_request::$name);
            random_request_transaction(requests)
        }
        create_read_from_file_transaction!($($rest)*);
    };
}

create_read_from_file_transaction! {
    get_block_with_tx_hashes_by_hash, "crates/papyrus_load_test/src/resources/block_hash.txt";
}
