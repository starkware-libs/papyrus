use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::string::String;

use assert::assert_ok;
use serde::{Deserialize, Serialize};

use super::transaction::DeployAccountTransaction;

// TODO(shahak): Remove code duplication with starknet_reader_client.
fn read_resource_file(path_in_resource_dir: &str) -> String {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    return read_to_string(path.to_str().unwrap()).unwrap();
}

fn validate_load_and_dump<TransactionType: for<'a> Deserialize<'a> + Serialize>(
    path_in_resource_dir: &str,
) {
    let json_str = read_resource_file(path_in_resource_dir);
    let load_result = serde_json::from_str::<TransactionType>(&json_str);
    assert_ok!(load_result);
    let dump_result = serde_json::to_value(&load_result.unwrap());
    assert_ok!(dump_result);
    let json_value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json_value, dump_result.unwrap());
}
