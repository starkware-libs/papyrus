use std::fs::File;
use std::io::{Write};

use test_utils::send_request;

pub async fn create_files(node_address: &str) {
    last_block_number(node_address).await;
}

// Creates the file last_block_number.txt. Write to the file the last block number for the load
// test.
async fn last_block_number(node_address: &str) {
    let last_block_answer = &send_request(node_address, "starknet_blockNumber", "").await["result"];
    let mut file =
        File::create("crates/papyrus_load_test/src/resources/last_block_number.txt").unwrap();
    file.write_all(last_block_answer.to_string().as_bytes()).unwrap();
}
