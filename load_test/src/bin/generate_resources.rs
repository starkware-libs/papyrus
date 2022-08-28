use std::collections::BTreeMap;
use std::fs::File;
use std::string::String;

use reqwest::Client;
use serde_json::to_writer;
use url::Url;

const GET_BLOCK_URL: &str = "feeder_gateway/get_block";
const BLOCK_NUMBER_QUERY: &str = "blockNumber";

#[tokio::main]
async fn main() {
    // TODO(shahak): Change these into executable arguments with default values that are the
    // current ones.
    const N_BLOCKS: u32 = 10;
    const URL: &str = "https://alpha4.starknet.io";
    const OUTPUT_PATH: &str = "resources.json";

    let client = Client::builder().build().unwrap();
    let base_url = Url::parse(URL).unwrap();
    let mut resources_map = BTreeMap::<String, String>::new();
    for block_number in 1..N_BLOCKS {
        let mut url = base_url.join(GET_BLOCK_URL).unwrap();
        url.query_pairs_mut().append_pair(BLOCK_NUMBER_QUERY, &block_number.to_string());
        let message = client.get(url.clone()).send().await.unwrap().text().await.unwrap();
        let key_url = base_url.make_relative(&url).unwrap();
        resources_map.insert(key_url.as_str().to_string(), message);
    }
    to_writer(&File::create(OUTPUT_PATH).unwrap(), &resources_map).unwrap();
}
