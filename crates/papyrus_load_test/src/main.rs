// This code is inspired by the pathfinder load test.
// To run this load test, run locally a node and then run:
//      cargo run -r -p papyrus_load_test -- -t 5m -H http://127.0.0.1:8080
// For more options run:
//      cargo run -r -p papyrus_load_test -- --help

use std::env;
use std::fs::File;

use goose::{util, GooseAttack};
use papyrus_load_test::scenarios::*;
use serde::Serialize;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let metrics = GooseAttack::initialize()?
        .register_scenario(general_request().set_weight(1)?)
        .register_scenario(serial_get_block().set_weight(1)?)
        .execute()
        .await?;

    // The OUTPUT_FILE env is expected to be a valid path in the os.
    // If exists, aggregated results will be written to that path in the following json format:
    // [
    //     {
    //         "name": <request name>,
    //         "units": "Milliseconds",
    //         "value": <request median time>,
    //     },
    // ]
    if let Ok(path) = env::var("OUTPUT_FILE") {
        let file = File::create(path)?;
        let mut performance: Vec<Entry> = vec![];
        for (name, data) in metrics.requests {
            let raw_data = data.raw_data;
            let median = util::median(
                &raw_data.times,
                raw_data.counter,
                raw_data.minimum_time,
                raw_data.maximum_time,
            );
            performance.push(Entry { name, units: "Milliseconds".to_string(), value: median });
        }
        serde_json::to_writer(file, &performance)?
    }

    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Entry {
    name: String,
    units: String, // "Milliseconds"
    value: usize,
}
