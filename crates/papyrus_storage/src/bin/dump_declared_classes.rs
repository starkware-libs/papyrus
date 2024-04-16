use std::cmp::min;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use clap::{Arg, Command};
use futures::future::join_all;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{from_value, json, Value};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::hash::StarkFelt;
use starknet_api::state::ContractClass;

const BATCH_SIZE: u64 = 100;
const TIMEOUT: Duration = Duration::from_secs(10);
const RETRIES: usize = 10;

#[tokio::main]
async fn main() {
    let cli_params = get_cli_params();
    let url = &cli_params.url;
    let request_builder = Client::new().post(url).timeout(TIMEOUT);
    let mut class_hashes = Vec::new();

    let mut first_in_batch = cli_params.start_block;
    while first_in_batch < cli_params.end_block {
        let last_block_in_batch = min(first_in_batch + BATCH_SIZE, cli_params.end_block);
        println!("Starting batch from block {} to {}", first_in_batch, last_block_in_batch);
        let mut futures = Vec::new();
        for block_number in first_in_batch..last_block_in_batch {
            futures.push(send_request_with_retries(
                get_state_update_by_number(block_number),
                &request_builder,
            ));
        }

        let state_diffs = join_all(futures).await;

        // Extract the class hashes from the responses.
        for sd in state_diffs {
            let declared_classes =
                sd["result"]["state_diff"]["declared_classes"].as_array().unwrap();
            for dc in declared_classes {
                let class_hash =
                    ClassHash(StarkFelt::try_from(dc["class_hash"].as_str().unwrap()).unwrap());
                let compiled_class_hash = CompiledClassHash(
                    StarkFelt::try_from(dc["compiled_class_hash"].as_str().unwrap()).unwrap(),
                );
                class_hashes.push((class_hash, compiled_class_hash));
            }
        }
        first_in_batch += BATCH_SIZE;
    }

    // Write the classes to the file.
    let mut file = File::create(&cli_params.file_path).unwrap();
    for current in class_hashes {
        println!("Getting class with hash: {}", current.0);
        let response = send_request_with_retries(
            get_class_by_number(current.0, cli_params.end_block),
            &request_builder,
        )
        .await["result"]
            .take();
        let contract = from_value::<ContractClass>(response).unwrap();

        let class = ClassEntry { class_hash: current.0, compiled_class_hash: current.1, contract };

        serde_json::to_writer(&mut file, &class).unwrap();
        file.write_all(b"\n").unwrap();
    }

    file.flush().unwrap();
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
struct ClassEntry {
    class_hash: ClassHash,
    compiled_class_hash: CompiledClassHash,
    contract: ContractClass,
}

#[allow(dead_code)]
fn read_classes_from_file(file_path: &str) -> Vec<ClassEntry> {
    let file = File::open(file_path).unwrap();
    let reader = BufReader::new(file);
    let mut classes = Vec::new();
    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let class: ClassEntry = serde_json::from_str(&line).expect("Failed to deserialize object");
        classes.push(class);
    }
    classes
}

pub fn jsonrpc_request(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": method,
        "params": params,
    })
}

pub fn get_state_update_by_number(block_number: u64) -> Value {
    jsonrpc_request("starknet_getStateUpdate", json!([{ "block_number": block_number }]))
}

pub fn get_class_by_number(class_hash: ClassHash, block_number: u64) -> Value {
    jsonrpc_request("starknet_getClass", json!([{ "block_number": block_number }, class_hash.0]))
}

pub async fn send_request_with_retries(request: Value, request_builder: &RequestBuilder) -> Value {
    for iteration in 0..RETRIES {
        // let res=request_builder.try_clone().unwrap().json(&request).send().await;
        let Ok(res) = request_builder.try_clone().unwrap().json(&request).send().await else {
            println!("Failed to send request in iteration {iteration}. Retrying...");
            continue;
        };

        if !res.status().is_success() {
            println!(
                "Failed to get successful response in iteration {iteration}, error code: {}. Retrying...", res.status()
            );
            continue;
        }

        let Ok(res) = res.json().await else {
            println!("Failed to get response value in iteration {iteration}. Retrying...");
            continue;
        };

        return res;
    }
    panic!(
        "Failed to get response after {} retries for the following request:\n{}",
        RETRIES, request
    );
}

struct CliParams {
    start_block: u64,
    end_block: u64,
    file_path: String,
    url: String,
}

/// The start_block and end_block arguments are mandatory and define the block range to dump,
/// start_block is inclusive and end_block is exclusive. The file_path is an optional parameter,
/// otherwise the data will be dumped to "dump_declared_classes.json".
fn get_cli_params() -> CliParams {
    let matches = Command::new("Dump declared classes")
        .arg(
            Arg::new("file_path")
                .short('f')
                .long("file_path")
                .default_value("dump_declared_classes.json")
                .help("The file path to dump the declared classes table to."),
        )
        .arg(
            Arg::new("start_block")
                .short('s')
                .long("start_block")
                .required(true)
                .help("The block number to start dumping from."),
        )
        .arg(
            Arg::new("end_block")
                .short('e')
                .long("end_block")
                .required(true)
                .help("The block number to end dumping at."),
        )
        .arg(Arg::new("url").short('u').long("url").required(true).help("A URL to RPC server."))
        .get_matches();

    let file_path =
        matches.get_one::<String>("file_path").expect("Failed parsing file_path").to_string();
    let start_block = matches
        .get_one::<String>("start_block")
        .expect("Failed parsing start_block")
        .parse::<u64>()
        .expect("Failed parsing start_block");
    let end_block = matches
        .get_one::<String>("end_block")
        .expect("Failed parsing end_block")
        .parse::<u64>()
        .expect("Failed parsing end_block");
    if start_block >= end_block {
        panic!("start_block must be smaller than end_block");
    }
    let url = matches.get_one::<String>("url").expect("Failed parsing url").to_string();
    CliParams { start_block, end_block, file_path, url }
}
