use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::string::String;

use assert::assert_ok;

use super::transaction::DeployAccountTransaction;

// TODO(shahak): Remove code duplication with starknet_reader_client.
fn read_resource_file(path_in_resource_dir: &str) -> String {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    return read_to_string(path.to_str().unwrap()).unwrap();
}
