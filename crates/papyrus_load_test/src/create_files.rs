use std::fs::File;
use std::io::{BufWriter, Write};
use std::net::SocketAddr;

use test_utils::send_request;

use crate::get_random_block_number;

pub async fn create_files(node_address: &str) {
    let node_socket = node_address.parse::<SocketAddr>().unwrap();
    last_block_number(node_socket).await;
    block_number(5);
}

// Creates the block_number.txt file. Write to the file lines_num blocks number.
fn block_number(lines_num: u32) {
    let file = File::create("crates/papyrus_load_test/src/resources/block_number.txt").unwrap();
    let mut writer = BufWriter::new(file);

    for _ in 0..lines_num - 1 {
        writer.write_all(get_random_block_number().to_string().as_bytes()).unwrap();
        writer.write_all("\n".as_bytes()).unwrap();
    }
    writer.write_all(get_random_block_number().to_string().as_bytes()).unwrap();
    writer.flush().unwrap();
}

// Creates the file last_block_number.txt. Write to the file the last block number for the load
// test.
async fn last_block_number(node_address: SocketAddr) {
    let last_block_answer = &send_request(node_address, "starknet_blockNumber", "").await["result"];
    let mut file =
        File::create("crates/papyrus_load_test/src/resources/last_block_number.txt").unwrap();
    file.write_all(last_block_answer.to_string().as_bytes()).unwrap();
}
