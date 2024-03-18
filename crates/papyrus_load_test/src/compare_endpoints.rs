use std::env::var;
use std::fs::{create_dir, remove_dir_all, File};
use std::io::Write;
use std::path::Path;

use reqwest::Client;
use serde_json::Value as jsonVal;

use crate::create_request;
use crate::transactions::create_requests_vector_from_file;

// TODO(dvir): consider using logging instead of printing.
// TODO(dvir): consider add tests for this.
// TODO(dvir): consider adding also test for endpoints without any parameters.

lazy_static::lazy_static! {
    // The URL of the first node.
    static ref ALPHA_NODE_URL: String = var("ALPHA_NODE_URL").unwrap();
    // The URL of the second node.
    static ref BETA_NODE_URL: String = var("BETA_NODE_URL").unwrap();

    // The path to the results directory.
    static ref RESULT_PATH: String = var("RESULT_PATH").unwrap_or("./results".to_string());
    // The name of the file to write to the request with different results.
    static ref REQUEST_FILE_NAME: String = var("REQUEST_FILE_NAME").unwrap_or("requests.txt".to_string());
    // The name of the file to write to the response of the alpha node.
    static ref ALPHA_RESPONSE_FILE_NAME: String = var("ALPHA_RESPONSE_FILE_NAME").unwrap_or("alpha_response.txt".to_string());
    // The name of the file to write to the response of the beta node.
    static ref BETA_RESPONSE_FILE_NAME: String = var("BETA_RESPONSE_FILE_NAME").unwrap_or("beta_response.txt".to_string());
}

// Maximum number of retries when sending a request.
const MAX_RETRIES: usize = 3;

// TODO(dvir): remove the order_arrays fix where we want to check also the order.
// Creates the tests for comparing different endpoints.
create_compare_endpoints_test! {
    get_block_with_transaction_hashes_by_number, "block_number.txt", &[order_arrays];
    get_block_with_transaction_hashes_by_hash, "block_hash.txt", &[order_arrays];
    get_block_with_full_transactions_by_number, "block_number.txt", &[order_arrays];
    get_block_with_full_transactions_by_hash, "block_hash.txt", &[order_arrays];
    get_block_transaction_count_by_number, "block_number.txt", &[order_arrays];
    get_block_transaction_count_by_hash, "block_hash.txt", &[order_arrays];
    get_state_update_by_number, "block_number.txt", &[order_arrays];
    get_state_update_by_hash, "block_hash.txt", &[order_arrays];
    get_transaction_by_block_id_and_index_by_number, "block_number_and_transaction_index.txt", &[order_arrays];
    get_transaction_by_block_id_and_index_by_hash, "block_hash_and_transaction_index.txt", &[order_arrays];
    get_transaction_by_hash, "transaction_hash.txt", &[order_arrays];
    get_transaction_receipt, "transaction_hash.txt", &[order_arrays];
    get_class_at_by_number, "block_number_and_contract_address.txt", &[order_arrays];
    get_class_at_by_hash, "block_hash_and_contract_address.txt", &[order_arrays];
    get_class_hash_at_by_number, "block_number_and_contract_address.txt", &[order_arrays];
    get_class_hash_at_by_hash, "block_hash_and_contract_address.txt", &[order_arrays];
    get_nonce_by_number, "block_number_and_contract_address.txt", &[order_arrays];
    get_nonce_by_hash, "block_hash_and_contract_address.txt", &[order_arrays];
    get_storage_at_by_number, "block_number_and_contract_address.txt", &[order_arrays];
    get_storage_at_by_hash, "block_hash_and_contract_address.txt", &[order_arrays];
    get_class_by_number, "block_number_and_class_hash.txt", &[order_arrays];
    get_class_by_hash, "block_hash_and_class_hash.txt", &[order_arrays];
    // The continuation_token format is not specified in the spec.
    get_events_with_address, "block_range_and_contract_address.txt", &[order_arrays, |val| remove_all_fields(val, "continuation_token")];
    get_events_without_address, "block_range_and_contract_address.txt", &[order_arrays, |val| remove_all_fields(val, "continuation_token")];
    trace_transaction, "transaction_hash.txt", &[order_arrays];
    trace_block_transactions_by_number, "block_number.txt", &[order_arrays];
    trace_block_transactions_by_hash, "block_hash.txt", &[order_arrays];
}

// TODO(dvir): consider making the requests concurrent.
// Compares the result of the requests for the endpoint after performing the fixes on the returned
// responses.
async fn compare(requests: Vec<jsonVal>, endpoint: &str, fixes: &[fn(&mut jsonVal)]) -> bool {
    // This variable is used to check if all the responses are the same.
    let mut everything_is_the_same = true;

    // Remove the current endpoint results if exists.
    let result_path = Path::new(&*RESULT_PATH);
    let endpoint_path = result_path.join(endpoint);
    if endpoint_path.exists() {
        remove_dir_all(&endpoint_path).unwrap();
    }

    let client = Client::new();
    for (idx, request) in requests.iter().enumerate() {
        if idx % 10 == 0 {
            println!("{endpoint} iteration: {idx}");
        }

        let Some(mut alpha_response) = send(&client, &ALPHA_NODE_URL, request).await else {
            // TODO(dvir): consider formatting this nicer.
            println!(
                "Failed to send request: {request} to URL: {} skipping this request.",
                *ALPHA_NODE_URL
            );
            continue;
        };
        let Some(mut beta_response) = send(&client, &BETA_NODE_URL, request).await else {
            println!(
                "Failed to send request {request} to {}, skipping this request.",
                *BETA_NODE_URL
            );
            continue;
        };

        for fix in fixes {
            fix(&mut alpha_response);
            fix(&mut beta_response);
        }

        if alpha_response != beta_response {
            println!(
                "Different responses for method: {} with parameters: {}",
                request["method"], request["params"]
            );

            if everything_is_the_same {
                everything_is_the_same = false;
                create_dir(&endpoint_path).unwrap();
            }

            // TODO(dvir): consider changing the directory name.
            // The path for the current request.
            let param_path = endpoint_path.join(request["params"].to_string());
            create_dir(&param_path).unwrap();

            let mut file = File::create(param_path.join(&*REQUEST_FILE_NAME)).unwrap();
            file.write_all(format!("{:#?}", request).as_bytes()).unwrap();

            let mut file = File::create(param_path.join(&*ALPHA_RESPONSE_FILE_NAME)).unwrap();
            file.write_all(format!("{:#?}", alpha_response).as_bytes()).unwrap();

            let mut file = File::create(param_path.join(&*BETA_RESPONSE_FILE_NAME)).unwrap();
            file.write_all(format!("{:#?}", beta_response).as_bytes()).unwrap();
        }
    }

    println!("Finished comparing {endpoint}.");
    everything_is_the_same
}

// Given [Name, "Path", &[fix1, fix2, ...];] writes the following test:
//      #[tokio::test]
//      async fn Name() {
//          let requests = create_requests_vector("Path", create_request::Name);
//          assert!(compare(requests, Name,  &[fix1, fix2, ...]).await);
//      }
macro_rules! create_compare_endpoints_test {
    () => {};
    ($name:ident, $file_name:literal, $fixes:expr; $($rest:tt)*) => {
        #[tokio::test]
        async fn $name (){
            let requests = create_requests_vector_from_file($file_name, create_request::$name);
            assert!(compare(requests, stringify!($name), $fixes).await);
        }
        create_compare_endpoints_test!($($rest)*);
    };
}
pub(crate) use create_compare_endpoints_test;

// Sends the request to the url and returns the response. If the request fails, retries up to
// MAX_RETRIES times.
async fn send(client: &Client, url: &str, req: &jsonVal) -> Option<jsonVal> {
    let builder = client.post(url).json(req);
    for _ in 0..MAX_RETRIES {
        let Ok(res) = builder.try_clone().unwrap().send().await else {
            continue;
        };
        let Ok(res) = res.json().await else {
            continue;
        };
        return Some(res);
    }
    None
}

// Removes all fields with name 'field_name'.
fn remove_all_fields(val: &mut jsonVal, field_name: &str) {
    match val {
        jsonVal::Array(vec) => {
            for entry in vec {
                remove_all_fields(entry, field_name);
            }
        }
        jsonVal::Object(map) => {
            map.remove(field_name);
            for (_key, val) in map {
                remove_all_fields(val, field_name);
            }
        }
        _ => {}
    }
}

// Orders all the arrays in val by their string representation.
// Useful to compare two json object that are the same but with different order.
fn order_arrays(val: &mut jsonVal) {
    match val {
        jsonVal::Array(vec) => {
            for entry in vec.iter_mut() {
                order_arrays(entry);
            }
            vec.sort_by_key(|val| val.to_string());
        }
        jsonVal::Object(map) => {
            for (_key, val) in map {
                order_arrays(val);
            }
        }
        _ => {}
    }
}
