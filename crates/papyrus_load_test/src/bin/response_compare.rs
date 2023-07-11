use std::fs::File;
use std::io::Write;

use papyrus_load_test::create_request::*;
use papyrus_load_test::transactions::create_requests_vector_from_file;
use serde_json::Value as jsonVal;

async fn send(client: &reqwest::Client, url: &str, req: &jsonVal) -> jsonVal {
    client.post(url).json(req).send().await.unwrap().json().await.unwrap()
}

// Removes the field in path. Returns the removed field value.
fn remove_field(val: &mut jsonVal, path: Vec<&str>) -> Option<jsonVal> {
    let mut path = path;
    let last = path.pop().unwrap();
    let mut mut_val = val;
    for f in path {
        mut_val = mut_val.get_mut(f).unwrap();
    }
    let removed = mut_val.as_object_mut().unwrap().remove(last);
    removed
}

// Removes all field with name 'field'.
fn remove_all_field(val: &mut jsonVal, field: &str) {
    match val {
        jsonVal::Array(x) => {
            for i in x {
                remove_all_field(i, field);
            }
        }
        jsonVal::Object(x) => {
            x.remove(field);
            for i in x {
                remove_all_field(i.1, field);
            }
        }
        _ => {}
    }
}

// Replaced all the fields with name "old" with a field with name "new" and the same value.
fn replace_all(val: &mut jsonVal, old: &str, new: &str) {
    match val {
        jsonVal::Array(vec) => {
            for i in vec {
                replace_all(i, old, new);
            }
        }
        jsonVal::Object(map) => {
            if let Some(val) = map.remove(old) {
                map.insert(new.to_string(), val);
            }
            for i in map {
                replace_all(i.1, old, new);
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
            for i in vec.iter_mut() {
                order_arrays(i);
            }
            vec.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        }
        jsonVal::Object(map) => {
            for i in map {
                order_arrays(i.1);
            }
        }
        _ => {}
    }
}

const PAPYRUS_URL: &str = "http://localhost:8080";
const OTHER_URL: &str = "http://localhost:9545";

// A function that takes arguments and returns a json rpc request.
const CREATE_REQUESTS: fn(&str) -> jsonVal = get_block_with_transaction_hashes_by_number;
// A file path with arguments to requests.
const FILE: &str = "block_number.txt";

#[tokio::main]
async fn main() {
    // A vector of json rpc requests.
    let reqs = create_requests_vector_from_file(FILE, CREATE_REQUESTS);

    let client = reqwest::Client::new();
    for idx in 0..reqs.len() {
        let cur_req = &reqs[idx];
        println!("idx: {}", idx);

        let mut our = send(&client, PAPYRUS_URL, &cur_req).await;
        let mut other = send(&client, OTHER_URL, &cur_req).await;

        // All kinds of changes to make the jsons more similar.
        {
            // remove_all_field(&mut our, "from_address");
            // remove_all_field(&mut other, "from_address");

            // remove_all_field(&mut our, "to_address");
            // remove_all_field(&mut other, "to_address");

            // remove_all_field(&mut our, "contract_address");
            // remove_all_field(&mut other, "contract_address");

            // replace_all(&mut other, "declared_contract_hashes", "declared_classes");
            // if our["result"]["state_diff"]["replaced_classes"].as_array().unwrap().is_empty(){
            //     remove_field(&mut our, vec!["result", "state_diff", "replaced_classes"]);
            // }
            // if our["result"]["state_diff"]["deprecated_declared_classes"].as_array().unwrap().
            // is_empty(){     remove_field(&mut our, vec!["result", "state_diff",
            // "deprecated_declared_classes"]); }

            // remove_all_field( &mut our, "declared_classes");
            // remove_all_field( &mut our, "deprecated_declared_classes");
            // remove_all_field( &mut other, "declared_contract_hashes");

            // remove_all_field( &mut our, "status");
            // remove_all_field( &mut other, "status");

            // remove_all_field( &mut our, "contract_address");
            // remove_all_field( &mut other, "contract_address");

            // remove_all_field( &mut our, "continuation_token");
            // remove_all_field( &mut other, "continuation_token");

            // remove_all_field( &mut our, "declared_classes");
            // remove_all_field( &mut other, "declared_contract_hashes");
        }

        order_arrays(&mut our);
        order_arrays(&mut other);

        if our != other {
            let mut f = File::create("request.txt").unwrap();
            f.write_all(format!("{:#?}", cur_req).as_bytes()).unwrap();

            let mut f = File::create("our.txt").unwrap();
            f.write_all(format!("{:#?}", our).as_bytes()).unwrap();

            let mut f = File::create("other.txt").unwrap();
            f.write_all(format!("{:#?}", other).as_bytes()).unwrap();

            println!("request:\n({:#?}\n", cur_req);
            println!("our:\n{:#?}\n", our);
            println!("other:\n{:#?}\n", other);

            println!("index: {}", idx);
            return;
        }
    }
    println!("Everything is the same!!!");
}
