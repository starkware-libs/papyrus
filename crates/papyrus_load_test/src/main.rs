// This code is inspired by the pathfinder load test.
// first set the env variable VERSION_ID to the version of the node you want to test.
// To run this load test, run locally a node and then run:
//      cargo run -r -p papyrus_load_test -- -t 5m -H http://127.0.0.1:8080 --scenarios=generalrequestv004
// To create the files of requests run:
//      cargo run -r -p papyrus_load_test -- --create_files 127.0.0.1:8080
// For more options run:
//      cargo run -r -p papyrus_load_test -- --help

use std::env;
use std::fs::File;

use assert_matches::assert_matches;
use goose::{util, GooseAttack};
use papyrus_load_test::create_files::create_files;
use papyrus_load_test::scenarios;
use serde::Serialize;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    assert_matches!(std::env::var("VERSION_ID"), Ok(_));
    if args.len() > 1 && args[1].eq("--create_files") {
        create_files(&args[2]).await;
        return Ok(());
    }

    let metrics = GooseAttack::initialize()?
        // The choice between V0_3 and V0_4 must be also in the environment variable VERSION_ID.
        .register_scenario(scenarios::general_request_v0_3())
        .register_scenario(scenarios::general_request_v0_4())
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
