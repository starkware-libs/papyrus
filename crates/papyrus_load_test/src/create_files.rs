use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;

use test_utils::send_request;

use crate::{get_random_block_number, path_in_resources};

// The number of block numbers to write.
// Currently, this number is random; we will decide how to choose this number
// in the future.
const BLOCK_NUMBER_COUNT: u32 = 5;

// Creates the files to run the load test.
pub async fn create_files(node_address: &str) {
    let node_socket = node_address.parse::<SocketAddr>().unwrap();
    last_block_number(node_socket).await;
    block_number(BLOCK_NUMBER_COUNT);
}

// Creates the block_number.txt file. Write to the file block_number_count random block numbers.
fn block_number(block_number_count: u32) {
    let mut to_write = String::new();
    for _ in 0..block_number_count {
        let block_number = get_random_block_number().to_string();
        to_write = to_write + &block_number + "\n";
    }
    // Remove the last '\n'.
    to_write.pop().expect("to_write String is empty, the block_number_nums is zero.");
    let mut file = File::create(path_in_resources("block_number.txt"))
        .expect("Create file \"block_number.txt\" failed.");
    file.write_all(to_write.as_bytes()).unwrap();
}

// Creates the file last_block_number.txt. Write to the file the last block number for the load
// test.
async fn last_block_number(node_address: SocketAddr) {
    let last_block_number = &send_request(node_address, "starknet_blockNumber", "").await["result"];
    let mut file = File::create(path_in_resources("last_block_number.txt")).unwrap();
    file.write_all(last_block_number.to_string().as_bytes()).unwrap();
}
