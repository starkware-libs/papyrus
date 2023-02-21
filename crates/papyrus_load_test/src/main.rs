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
        .register_scenario(block_by_number())
        .register_scenario(block_by_hash())
        .register_scenario(general_request())
        .execute()
        .await?;

    // The OUTPUT_FILE env is expected to be a valid path in the os.
    // If exists, aggregated results will be written to that path in the following json format:
    // [
    //     {
    //         "name": <scenario name>,
    //         "units": "Milliseconds",
    //         "value": <scenario median time>,
    //     },
    // ]
    if let Ok(path) = env::var("OUTPUT_FILE") {
        let file = File::create(path)?;
        let mut data: Vec<Entry> = vec![];
        for scenario in metrics.scenarios {
            let median = util::median(
                &scenario.times,
                scenario.counter,
                scenario.min_time,
                scenario.max_time,
            );
            data.push(Entry {
                name: scenario.name,
                units: "Milliseconds".to_string(),
                value: median,
            });
        }
        serde_json::to_writer(file, &data)?
    }

    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Entry {
    name: String,
    units: String, // "Milliseconds"
    value: usize,
}
