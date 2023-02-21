use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::sync::Arc;

use goose::goose::{Transaction, TransactionFunction};
use rand::Rng;
use serde_json::Value as jsonVal;

use crate::{create_request, post_jsonrpc_request};
pub type TransactionsResult = Result<Transaction, TransactionsError>;

#[derive(thiserror::Error, Debug)]
pub enum TransactionsError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

fn create_requests_vector(
    path: &str,
    convert_to_request: fn(String) -> jsonVal,
) -> Result<Vec<jsonVal>, TransactionsError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut requests = Vec::<jsonVal>::new();
    for line in reader.lines() {
        requests.push(convert_to_request(line?));
    }
    Ok(requests)
}

// Given [Name, "Path";] write the function:
// pub fn Name() -> TransactionsResult {
//     let requests = create_requests_vector("Path", create_request::Name)?;
//     Ok(random_request_transaction(requests))
// }
macro_rules! create_read_from_file_transaction {
    () => {};
    ($name:tt, $file_name:literal; $($rest:tt)*) => {
        pub fn $name() -> TransactionsResult {
            let requests = create_requests_vector($file_name, create_request::$name)?;
            Ok(random_request_transaction(requests))
        }
        create_read_from_file_transaction!($($rest)*);
    };
}

create_read_from_file_transaction! {
    get_block_with_tx_hashes_by_number, "crates/papyrus_load_test/src/resources/block_number.txt";
    get_block_with_tx_hashes_by_hash,   "crates/papyrus_load_test/src/resources/block_hash.txt";
}

pub fn serial_get_block(start: u32, end: u32) -> Transaction {
    let func: TransactionFunction = Arc::new(move |user| {
        Box::pin(async move {
            for block_number in start..end {
                let request =
                    create_request::get_block_with_tx_hashes_by_number(block_number.to_string());
                post_jsonrpc_request(user, &request).await?;
            }
            Ok(())
        })
    });
    Transaction::new(func).set_name("serial_get_block")
}
